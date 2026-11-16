//! 本地需求文档解析。
//!
//! 支持 Markdown、纯文本、PDF 和 DOCX，并为可引用片段生成稳定锚点；读取前先
//! 校验扩展名与大小，避免把二进制或超限内容送入模型。

use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

use coldstart_core::{ArtifactFormat, DocumentAnchor};
use encoding_rs::{GB18030, UTF_16BE, UTF_16LE};
use quick_xml::{Reader, events::Event};
use regex::Regex;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

pub const MAX_DOCUMENT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_EXTRACTED_CHARACTERS: usize = 500_000;
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_IMAGES_PER_DOCUMENT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub format: ArtifactFormat,
    pub text: String,
    pub anchors: Vec<DocumentAnchor>,
    pub images: Vec<ExtractedImage>,
    pub content_hash: String,
    pub text_hash: String,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedImage {
    pub id: String,
    pub anchor_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported document format: {0}")]
    UnsupportedFormat(String),
    #[error("document exceeds the {limit} byte limit")]
    TooLarge { limit: u64 },
    #[error("document text exceeds the {limit} character limit")]
    TextTooLarge { limit: usize },
    #[error("document is not valid text: {0}")]
    InvalidEncoding(String),
    #[error("invalid DOCX document: {0}")]
    InvalidDocx(String),
    #[error("PDF extraction failed: {0}")]
    Pdf(String),
    #[error("document contains no extractable text")]
    NoExtractableText,
}

impl DocumentError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "document_io",
            Self::UnsupportedFormat(_) => "unsupported_document",
            Self::TooLarge { .. } => "document_too_large",
            Self::TextTooLarge { .. } => "document_text_too_large",
            Self::InvalidEncoding(_) => "invalid_document_encoding",
            Self::InvalidDocx(_) => "invalid_docx",
            Self::Pdf(_) => "pdf_extraction_failed",
            Self::NoExtractableText => "document_empty",
        }
    }
}

/// 根据扩展名选择解析器，校验文件大小并生成可溯源锚点；不支持或空文档显式失败。
pub fn extract_document(path: &Path) -> Result<ExtractedDocument, DocumentError> {
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(ArtifactFormat::from_extension)
        .ok_or_else(|| {
            DocumentError::UnsupportedFormat(
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default()
                    .to_owned(),
            )
        })?;
    let byte_size = fs::metadata(path)?.len();
    if byte_size > MAX_DOCUMENT_BYTES {
        return Err(DocumentError::TooLarge {
            limit: MAX_DOCUMENT_BYTES,
        });
    }
    let bytes = fs::read(path)?;
    let content_hash = sha256_hex(&bytes);
    let (text, anchors, images) = match format {
        ArtifactFormat::Markdown => {
            let text = decode_text(&bytes)?;
            let normalized = normalize_line_endings(&text);
            let anchors = extract_markdown_anchors(&normalized);
            let images = extract_markdown_images(path, &normalized, &anchors)?;
            (normalized.trim().to_owned(), anchors, images)
        }
        ArtifactFormat::Text => {
            let text = decode_text(&bytes)?;
            let normalized = normalize_line_endings(&text);
            let anchors = extract_text_anchors(&normalized);
            (normalized.trim().to_owned(), anchors, Vec::new())
        }
        ArtifactFormat::Docx => extract_docx(&bytes)?,
        ArtifactFormat::Pdf => {
            let (text, anchors) = extract_pdf(&bytes)?;
            (text, anchors, Vec::new())
        }
    };
    if text.trim().is_empty() || anchors.is_empty() {
        return Err(DocumentError::NoExtractableText);
    }
    if text.chars().count() > MAX_EXTRACTED_CHARACTERS {
        return Err(DocumentError::TextTooLarge {
            limit: MAX_EXTRACTED_CHARACTERS,
        });
    }
    let text_hash = sha256_hex(text.as_bytes());

    Ok(ExtractedDocument {
        format,
        text,
        anchors,
        images,
        content_hash,
        text_hash,
        byte_size,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

fn decode_text(bytes: &[u8]) -> Result<String, DocumentError> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(rest)
            .map(str::to_owned)
            .map_err(|error| DocumentError::InvalidEncoding(error.to_string()));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (decoded, _, had_errors) = UTF_16LE.decode(rest);
        if had_errors {
            return Err(DocumentError::InvalidEncoding("UTF-16LE".to_owned()));
        }
        return Ok(decoded.into_owned());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (decoded, _, had_errors) = UTF_16BE.decode(rest);
        if had_errors {
            return Err(DocumentError::InvalidEncoding("UTF-16BE".to_owned()));
        }
        return Ok(decoded.into_owned());
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_owned());
    }
    let (decoded, _, had_errors) = GB18030.decode(bytes);
    if had_errors {
        return Err(DocumentError::InvalidEncoding(
            "UTF-8 and GB18030 decoding both failed".to_owned(),
        ));
    }
    Ok(decoded.into_owned())
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn extract_markdown_anchors(text: &str) -> Vec<DocumentAnchor> {
    let mut anchors = Vec::new();
    let mut label = None;
    let mut lines = Vec::<String>::new();
    let mut section_number = 0usize;

    for line in text.lines() {
        if let Some(heading) = markdown_heading(line) {
            flush_markdown_anchor(&mut anchors, &mut label, &mut lines, &mut section_number);
            label = Some(heading.clone());
            lines.push(heading);
        } else {
            lines.push(line.to_owned());
        }
    }
    flush_markdown_anchor(&mut anchors, &mut label, &mut lines, &mut section_number);
    anchors
}

fn extract_markdown_images(
    document_path: &Path,
    text: &str,
    anchors: &[DocumentAnchor],
) -> Result<Vec<ExtractedImage>, DocumentError> {
    let parent = document_path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = fs::canonicalize(parent)?;
    let image_pattern = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)")
        .map_err(|error| DocumentError::InvalidEncoding(error.to_string()))?;
    let mut section_number = 0usize;
    let mut anchor_id = "preamble".to_owned();
    let mut images = Vec::new();
    let mut total_bytes = 0u64;

    for line in text.lines() {
        if markdown_heading(line).is_some() {
            section_number += 1;
            anchor_id = format!("section_{section_number}");
        }
        for capture in image_pattern.captures_iter(line) {
            if images.len() >= MAX_IMAGES_PER_DOCUMENT {
                return Ok(images);
            }
            if !anchors.iter().any(|anchor| anchor.id == anchor_id) {
                continue;
            }
            let target = capture[2]
                .trim()
                .trim_matches(|character| character == '<' || character == '>')
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if target.is_empty()
                || target.contains("://")
                || target.starts_with("data:")
                || target.starts_with('#')
            {
                continue;
            }
            let target_path = PathBuf::from(target);
            if target_path.is_absolute() {
                continue;
            }
            let resolved = parent.join(&target_path);
            let Ok(canonical) = fs::canonicalize(&resolved) else {
                continue;
            };
            if !canonical.starts_with(&canonical_parent) || !canonical.is_file() {
                continue;
            }
            let metadata = fs::metadata(&canonical)?;
            if metadata.len() > MAX_IMAGE_BYTES
                || total_bytes.saturating_add(metadata.len()) > MAX_IMAGE_BYTES
            {
                continue;
            }
            let bytes = fs::read(&canonical)?;
            let Some(mime_type) = detect_image_mime(&bytes) else {
                continue;
            };
            total_bytes = total_bytes.saturating_add(bytes.len() as u64);
            let file_name = canonical
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("image")
                .to_owned();
            images.push(ExtractedImage {
                id: format!("image_{}", images.len() + 1),
                anchor_id: anchor_id.clone(),
                file_name,
                mime_type: mime_type.to_owned(),
                sha256: sha256_hex(&bytes),
                bytes,
            });
        }
    }
    Ok(images)
}

fn detect_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn markdown_heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let level = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let title = rest.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

fn flush_markdown_anchor(
    anchors: &mut Vec<DocumentAnchor>,
    label: &mut Option<String>,
    lines: &mut Vec<String>,
    section_number: &mut usize,
) {
    let text = lines.join("\n").trim().to_owned();
    lines.clear();
    if text.is_empty() {
        *label = None;
        return;
    }
    let (id, anchor_label) = if let Some(label) = label.take() {
        *section_number += 1;
        (format!("section_{section_number}"), label)
    } else {
        ("preamble".to_owned(), "开篇".to_owned())
    };
    anchors.push(DocumentAnchor {
        id,
        label: anchor_label,
        text,
    });
}

fn extract_text_anchors(text: &str) -> Vec<DocumentAnchor> {
    let mut paragraphs = Vec::new();
    let mut current = Vec::<&str>::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            let paragraph = current.join("\n").trim().to_owned();
            if !paragraph.is_empty() {
                paragraphs.push(paragraph);
            }
            current.clear();
        } else {
            current.push(line);
        }
    }
    paragraphs
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let label = truncate_label(text.lines().next().unwrap_or_default(), 36);
            DocumentAnchor {
                id: format!("paragraph_{}", index + 1),
                label,
                text,
            }
        })
        .collect()
}

fn truncate_label(value: &str, max: usize) -> String {
    let mut label = value.trim().chars().take(max).collect::<String>();
    if value.trim().chars().count() > max {
        label.push('…');
    }
    label
}

fn extract_docx(
    bytes: &[u8],
) -> Result<(String, Vec<DocumentAnchor>, Vec<ExtractedImage>), DocumentError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
    let xml = read_docx_entry(&mut archive, "word/document.xml")?;
    let paragraphs = parse_docx_paragraphs(&xml)?;
    let text = paragraphs
        .iter()
        .map(|paragraph| paragraph.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let (anchors, placements) = if paragraphs.iter().any(|paragraph| paragraph.is_heading) {
        group_docx_content(&paragraphs)
    } else {
        let anchors = extract_text_anchors(&text);
        let placements = paragraph_image_placements(&paragraphs);
        (anchors, placements)
    };
    let relationships = read_docx_image_relationships(&mut archive)?;
    let images = read_docx_images(&mut archive, &relationships, placements)?;
    Ok((text, anchors, images))
}

fn read_docx_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<String, DocumentError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
    Ok(content)
}

fn read_docx_image_relationships(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<HashMap<String, String>, DocumentError> {
    let Ok(xml) = read_docx_entry(archive, "word/_rels/document.xml.rels") else {
        return Ok(HashMap::new());
    };
    let mut reader = Reader::from_str(&xml);
    let mut buffer = Vec::new();
    let mut relationships = HashMap::new();
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
        match event {
            Event::Start(start) | Event::Empty(start)
                if start.name().local_name().as_ref() == "Relationship" =>
            {
                let Some(id) = docx_attribute(&start, "Id") else {
                    buffer.clear();
                    continue;
                };
                if let Some(target) = docx_attribute(&start, "Target")
                    .and_then(|target| normalize_docx_media_target(&target))
                {
                    relationships.insert(id, target);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(relationships)
}

fn normalize_docx_media_target(target: &str) -> Option<String> {
    let normalized = target.replace('\\', "/");
    let relative = normalized.trim_start_matches('/');
    let relative = relative.strip_prefix("word/").unwrap_or(relative);
    if !relative.starts_with("media/")
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(relative.to_owned())
}

fn read_docx_images(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    relationships: &HashMap<String, String>,
    placements: Vec<DocxImagePlacement>,
) -> Result<Vec<ExtractedImage>, DocumentError> {
    let mut images = Vec::new();
    let mut total_bytes = 0u64;
    for placement in placements {
        if images.len() >= MAX_IMAGES_PER_DOCUMENT {
            break;
        }
        let Some(target) = relationships.get(&placement.relationship_id) else {
            continue;
        };
        let entry_name = format!("word/{target}");
        let Ok(entry) = archive.by_name(&entry_name) else {
            continue;
        };
        if entry.size() > MAX_IMAGE_BYTES {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
        if bytes.len() as u64 > MAX_IMAGE_BYTES
            || total_bytes.saturating_add(bytes.len() as u64) > MAX_IMAGE_BYTES
        {
            continue;
        }
        let Some(mime_type) = detect_image_mime(&bytes) else {
            continue;
        };
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        let file_name = target.rsplit('/').next().unwrap_or("image").to_owned();
        images.push(ExtractedImage {
            id: format!("image_{}", images.len() + 1),
            anchor_id: placement.anchor_id,
            file_name,
            mime_type: mime_type.to_owned(),
            sha256: sha256_hex(&bytes),
            bytes,
        });
    }
    Ok(images)
}

struct DocxImagePlacement {
    relationship_id: String,
    anchor_id: String,
}

struct DocxParagraph {
    text: String,
    is_heading: bool,
    image_relationships: Vec<String>,
}

fn parse_docx_paragraphs(xml: &str) -> Result<Vec<DocxParagraph>, DocumentError> {
    let mut reader = Reader::from_str(xml);
    let mut buffer = Vec::new();
    let mut paragraphs = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<String> = None;
    let mut current_images = Vec::<String>::new();
    let mut in_paragraph = false;
    let mut in_text = false;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
        match event {
            Event::Start(start) => match start.name().local_name().as_ref() {
                "p" => {
                    in_paragraph = true;
                    current_text.clear();
                    current_style = None;
                    current_images.clear();
                }
                "pStyle" if in_paragraph => {
                    current_style = docx_attribute(&start, "val");
                }
                "t" if in_paragraph => in_text = true,
                "blip" | "imagedata" if in_paragraph => {
                    if let Some(id) = docx_image_relationship(&start) {
                        current_images.push(id);
                    }
                }
                _ => {}
            },
            Event::Empty(empty) => {
                if in_paragraph {
                    match empty.name().local_name().as_ref() {
                        "pStyle" => current_style = docx_attribute(&empty, "val"),
                        "blip" | "imagedata" => {
                            if let Some(id) = docx_image_relationship(&empty) {
                                current_images.push(id);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Text(text) if in_text => {
                current_text.push_str(text.xml10_content().as_ref());
            }
            Event::End(end) => match end.name().local_name().as_ref() {
                "t" => in_text = false,
                "p" => {
                    in_paragraph = false;
                    in_text = false;
                    let text = normalize_inline_text(&current_text);
                    if !text.is_empty() || !current_images.is_empty() {
                        paragraphs.push(DocxParagraph {
                            text,
                            is_heading: current_style.as_deref().is_some_and(is_docx_heading_style),
                            image_relationships: std::mem::take(&mut current_images),
                        });
                    }
                    current_text.clear();
                    current_style = None;
                    current_images.clear();
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(paragraphs)
}

fn docx_image_relationship(start: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    docx_attribute(start, "embed").or_else(|| docx_attribute(start, "id"))
}

fn docx_attribute(start: &quick_xml::events::BytesStart<'_>, local_name: &str) -> Option<String> {
    for attribute in start.attributes().flatten() {
        if attribute.key.local_name().as_ref() == local_name {
            let value = attribute.value;
            let fallback = value.to_string();
            let decoded = quick_xml::escape::unescape(&value)
                .map(|decoded| decoded.into_owned())
                .unwrap_or(fallback);
            return Some(decoded.trim().to_owned());
        }
    }
    None
}

fn is_docx_heading_style(style: &str) -> bool {
    let normalized = style.to_ascii_lowercase();
    normalized.starts_with("heading") || normalized.contains("标题")
}

fn normalize_inline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn group_docx_content(
    paragraphs: &[DocxParagraph],
) -> (Vec<DocumentAnchor>, Vec<DocxImagePlacement>) {
    let mut anchors = Vec::new();
    let mut placements = Vec::new();
    let mut label: Option<String> = None;
    let mut lines = Vec::<String>::new();
    let mut images = Vec::<String>::new();
    let mut section_number = 0usize;

    for paragraph in paragraphs {
        if paragraph.is_heading {
            flush_docx_content(
                &mut anchors,
                &mut placements,
                &mut label,
                &mut lines,
                &mut images,
                &mut section_number,
            );
            label = Some(paragraph.text.clone());
            lines.push(paragraph.text.clone());
        } else {
            lines.push(paragraph.text.clone());
        }
        images.extend(paragraph.image_relationships.iter().cloned());
    }
    flush_docx_content(
        &mut anchors,
        &mut placements,
        &mut label,
        &mut lines,
        &mut images,
        &mut section_number,
    );
    (anchors, placements)
}

fn flush_docx_content(
    anchors: &mut Vec<DocumentAnchor>,
    placements: &mut Vec<DocxImagePlacement>,
    label: &mut Option<String>,
    lines: &mut Vec<String>,
    images: &mut Vec<String>,
    section_number: &mut usize,
) {
    let text = lines.join("\n").trim().to_owned();
    lines.clear();
    if text.is_empty() {
        images.clear();
        *label = None;
        return;
    }
    let (id, anchor_label) = if let Some(label) = label.take() {
        *section_number += 1;
        (format!("section_{section_number}"), label)
    } else {
        ("preamble".to_owned(), "开篇".to_owned())
    };
    placements.extend(images.drain(..).map(|relationship_id| DocxImagePlacement {
        relationship_id,
        anchor_id: id.clone(),
    }));
    anchors.push(DocumentAnchor {
        id,
        label: anchor_label,
        text,
    });
}

fn paragraph_image_placements(paragraphs: &[DocxParagraph]) -> Vec<DocxImagePlacement> {
    let mut placements = Vec::new();
    let mut paragraph_number = 0usize;
    let mut anchor_id = "paragraph_1".to_owned();
    for paragraph in paragraphs {
        if !paragraph.text.trim().is_empty() {
            paragraph_number += 1;
            anchor_id = format!("paragraph_{paragraph_number}");
        }
        placements.extend(
            paragraph
                .image_relationships
                .iter()
                .cloned()
                .map(|relationship_id| DocxImagePlacement {
                    relationship_id,
                    anchor_id: anchor_id.clone(),
                }),
        );
    }
    placements
}

fn extract_pdf(bytes: &[u8]) -> Result<(String, Vec<DocumentAnchor>), DocumentError> {
    let pages = catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem_by_pages(bytes)
    }))
    .map_err(|_| DocumentError::Pdf("PDF parser panicked".to_owned()))?
    .map_err(|error| DocumentError::Pdf(error.to_string()))?;
    let mut anchors = Vec::new();
    let mut page_texts = Vec::new();
    for (index, page) in pages.into_iter().enumerate() {
        let text = normalize_line_endings(&page);
        if text.is_empty() {
            continue;
        }
        page_texts.push(text.clone());
        anchors.push(DocumentAnchor {
            id: format!("page_{}", index + 1),
            label: format!("第 {} 页", index + 1),
            text,
        });
    }
    if anchors.is_empty() {
        return Err(DocumentError::NoExtractableText);
    }
    Ok((page_texts.join("\n\n"), anchors))
}
