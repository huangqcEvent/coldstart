//! 本地需求文档解析。
//!
//! 支持 Markdown、纯文本、PDF 和 DOCX，并为可引用片段生成稳定锚点；读取前先
//! 校验扩展名与大小，避免把二进制或超限内容送入模型。

use std::{
    fs,
    io::{Cursor, Read},
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
};

use coldstart_core::{ArtifactFormat, DocumentAnchor};
use encoding_rs::{GB18030, UTF_16BE, UTF_16LE};
use quick_xml::{Reader, events::Event};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

pub const MAX_DOCUMENT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_EXTRACTED_CHARACTERS: usize = 500_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocument {
    pub format: ArtifactFormat,
    pub text: String,
    pub anchors: Vec<DocumentAnchor>,
    pub content_hash: String,
    pub text_hash: String,
    pub byte_size: u64,
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
    let (text, anchors) = match format {
        ArtifactFormat::Markdown => {
            let text = decode_text(&bytes)?;
            let normalized = normalize_line_endings(&text);
            let anchors = extract_markdown_anchors(&normalized);
            (normalized.trim().to_owned(), anchors)
        }
        ArtifactFormat::Text => {
            let text = decode_text(&bytes)?;
            let normalized = normalize_line_endings(&text);
            let anchors = extract_text_anchors(&normalized);
            (normalized.trim().to_owned(), anchors)
        }
        ArtifactFormat::Docx => extract_docx(&bytes)?,
        ArtifactFormat::Pdf => {
            let (text, anchors) = extract_pdf(&bytes)?;
            (text, anchors)
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

fn extract_docx(bytes: &[u8]) -> Result<(String, Vec<DocumentAnchor>), DocumentError> {
    let archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
    let mut archive = archive;
    let mut document = archive
        .by_name("word/document.xml")
        .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
    let mut xml = String::new();
    document
        .read_to_string(&mut xml)
        .map_err(|error| DocumentError::InvalidDocx(error.to_string()))?;
    let paragraphs = parse_docx_paragraphs(&xml)?;
    let text = paragraphs
        .iter()
        .map(|paragraph| paragraph.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if paragraphs.iter().any(|paragraph| paragraph.is_heading) {
        Ok((text.clone(), group_docx_anchors(&paragraphs)))
    } else {
        Ok((text.clone(), extract_text_anchors(&text)))
    }
}

struct DocxParagraph {
    text: String,
    is_heading: bool,
}

fn parse_docx_paragraphs(xml: &str) -> Result<Vec<DocxParagraph>, DocumentError> {
    let mut reader = Reader::from_str(xml);
    let mut buffer = Vec::new();
    let mut paragraphs = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<String> = None;
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
                }
                "pStyle" if in_paragraph => {
                    current_style = docx_attribute(&start, "val");
                }
                "t" if in_paragraph => in_text = true,
                _ => {}
            },
            Event::Empty(empty) => {
                if empty.name().local_name().as_ref() == "pStyle" && in_paragraph {
                    current_style = docx_attribute(&empty, "val");
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
                    if !text.is_empty() {
                        paragraphs.push(DocxParagraph {
                            text,
                            is_heading: current_style.as_deref().is_some_and(is_docx_heading_style),
                        });
                    }
                    current_text.clear();
                    current_style = None;
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

fn group_docx_anchors(paragraphs: &[DocxParagraph]) -> Vec<DocumentAnchor> {
    let mut anchors = Vec::new();
    let mut label: Option<String> = None;
    let mut lines = Vec::<String>::new();
    let mut section_number = 0usize;

    for paragraph in paragraphs {
        if paragraph.is_heading {
            flush_docx_anchor(&mut anchors, &mut label, &mut lines, &mut section_number);
            label = Some(paragraph.text.clone());
            lines.push(paragraph.text.clone());
        } else {
            lines.push(paragraph.text.clone());
        }
    }
    flush_docx_anchor(&mut anchors, &mut label, &mut lines, &mut section_number);
    anchors
}

fn flush_docx_anchor(
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
