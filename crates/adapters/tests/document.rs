use std::{
    fs,
    io::{Cursor, Write},
};

use coldstart_adapters::{DocumentError, extract_document};
use coldstart_core::ArtifactFormat;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn fixture_dir() -> TempDir {
    tempfile::tempdir().expect("temporary directory must be created")
}

fn extract_fixture(name: &str, bytes: &[u8]) -> coldstart_adapters::ExtractedDocument {
    let directory = fixture_dir();
    let path = directory.path().join(name);
    fs::write(&path, bytes).expect("fixture must be written");
    extract_document(&path).expect("fixture must be extractable")
}

const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\ncoldstart-image";

#[test]
fn extracts_markdown_sections_with_heading_anchors() {
    let document = extract_fixture(
        "requirements.md",
        "# 订单系统\n\n实现订单管理。\n\n## 小程序\n\n支持微信登录。\n".as_bytes(),
    );

    assert_eq!(document.format, ArtifactFormat::Markdown);
    assert_eq!(document.anchors.len(), 2);
    assert_eq!(document.anchors[0].id, "section_1");
    assert_eq!(document.anchors[0].label, "订单系统");
    assert!(document.anchors[0].text.contains("订单"));
    assert_eq!(document.anchors[1].id, "section_2");
    assert_eq!(document.anchors[1].label, "小程序");
    assert!(!document.content_hash.is_empty());
    assert!(!document.text_hash.is_empty());
}

#[test]
fn keeps_markdown_preamble_before_the_first_heading() {
    let document = extract_fixture(
        "preamble.md",
        "这是项目背景。\n\n# 业务范围\n\n订单履约。\n".as_bytes(),
    );

    assert_eq!(document.anchors[0].id, "preamble");
    assert!(document.anchors[0].text.contains("项目背景"));
    assert_eq!(document.anchors[1].id, "section_1");
}

#[test]
fn decodes_txt_as_utf8_with_bom() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("第一段需求。\n\n第二段需求。\n".as_bytes());
    let document = extract_fixture("requirements.txt", &bytes);

    assert_eq!(document.format, ArtifactFormat::Text);
    assert_eq!(document.anchors.len(), 2);
    assert_eq!(document.anchors[0].id, "paragraph_1");
    assert!(document.text.starts_with("第一段需求"));
    assert!(!document.text.contains('\u{feff}'));
}

#[test]
fn falls_back_to_gb18030_for_legacy_txt() {
    let (encoded, _, had_errors) =
        encoding_rs::GB18030.encode("订单系统需要接入阿里云 OSS。\n\n小程序需要提审。");
    assert!(!had_errors);
    let document = extract_fixture("legacy.txt", &encoded);

    assert!(document.text.contains("订单系统"));
    assert!(document.text.contains("阿里云 OSS"));
    assert_eq!(document.anchors.len(), 2);
}

#[test]
fn extracts_docx_headings_and_paragraphs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>订单系统</w:t></w:r></w:p>
    <w:p><w:r><w:t>前台使用 App。</w:t></w:r></w:p>
    <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>发布要求</w:t></w:r></w:p>
    <w:p><w:r><w:t>需要小程序提审。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("word/document.xml", SimpleFileOptions::default())
        .expect("zip entry must start");
    writer
        .write_all(xml.as_bytes())
        .expect("document XML must be written");
    let bytes = writer.finish().expect("zip must finish").into_inner();
    let document = extract_fixture("requirements.docx", &bytes);

    assert_eq!(document.format, ArtifactFormat::Docx);
    assert_eq!(document.anchors.len(), 2);
    assert_eq!(document.anchors[0].id, "section_1");
    assert_eq!(document.anchors[0].label, "订单系统");
    assert!(document.anchors[0].text.contains("前台使用 App"));
    assert!(document.anchors[1].text.contains("小程序提审"));
}

#[test]
fn extracts_docx_embedded_images_and_links_them_to_section_anchors() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>订单原型</w:t></w:r></w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <a:blip r:embed="rId5"/>
        </w:drawing>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let relationships = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId5"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
    Target="media/image1.png"/>
</Relationships>"#;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in [
        ("word/document.xml", xml.as_bytes()),
        ("word/_rels/document.xml.rels", relationships.as_bytes()),
        ("word/media/image1.png", PNG_BYTES),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("zip entry must start");
        writer
            .write_all(contents)
            .expect("zip entry must be written");
    }
    let bytes = writer.finish().expect("zip must finish").into_inner();
    let document = extract_fixture("requirements.docx", &bytes);

    assert_eq!(document.images.len(), 1);
    let image = &document.images[0];
    assert_eq!(image.anchor_id, "section_1");
    assert_eq!(image.file_name, "image1.png");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.bytes, PNG_BYTES);
}

#[test]
fn extracts_markdown_images_from_the_document_directory() {
    let directory = fixture_dir();
    let images = directory.path().join("images");
    fs::create_dir(&images).expect("image directory must be created");
    fs::write(images.join("prototype.png"), PNG_BYTES).expect("image must be written");
    let path = directory.path().join("requirements.md");
    fs::write(
        &path,
        "# 订单原型\n\n![订单列表](images/prototype.png)\n\n## 状态\n\n待付款。\n",
    )
    .expect("markdown must be written");

    let document = extract_document(&path).expect("markdown must be extractable");

    assert_eq!(document.images.len(), 1);
    assert_eq!(document.images[0].anchor_id, "section_1");
    assert_eq!(document.images[0].file_name, "prototype.png");
    assert_eq!(document.images[0].mime_type, "image/png");
    assert_eq!(document.images[0].bytes, PNG_BYTES);
}

#[test]
fn ignores_markdown_images_outside_the_document_directory() {
    let directory = fixture_dir();
    let outside = directory.path().join("outside.png");
    fs::write(&outside, PNG_BYTES).expect("outside image must be written");
    let document_directory = directory.path().join("docs");
    fs::create_dir(&document_directory).expect("document directory must be created");
    let path = document_directory.join("requirements.md");
    fs::write(&path, "# 原型\n\n![越界](../outside.png)\n").expect("markdown must be written");

    let document = extract_document(&path).expect("markdown must remain importable");

    assert!(document.images.is_empty());
}

#[test]
fn rejects_pdf_without_extractable_text() {
    let bytes = build_pdf("");
    let directory = fixture_dir();
    let path = directory.path().join("scanned.pdf");
    fs::write(&path, bytes).expect("PDF must be written");

    let error = extract_document(&path).expect_err("empty PDF must fail");
    assert!(matches!(error, DocumentError::NoExtractableText));
    assert_eq!(error.code(), "document_empty");
}

#[test]
fn extracts_text_pdf_with_page_anchors() {
    let bytes = build_pdf("BT /F1 18 Tf 72 720 Td (ColdStart order system) Tj ET");
    let document = extract_fixture("requirements.pdf", &bytes);

    assert_eq!(document.format, ArtifactFormat::Pdf);
    assert_eq!(document.anchors.len(), 1);
    assert_eq!(document.anchors[0].id, "page_1");
    assert_eq!(document.anchors[0].label, "第 1 页");
    assert!(document.anchors[0].text.contains("ColdStart"));
}

#[test]
fn rejects_unsupported_extensions_and_empty_text_documents() {
    let directory = fixture_dir();
    let unsupported = directory.path().join("requirements.rtf");
    fs::write(&unsupported, b"content").expect("fixture must be written");
    let error = extract_document(&unsupported).expect_err("RTF must be unsupported");
    assert_eq!(error.code(), "unsupported_document");

    let empty = directory.path().join("empty.md");
    fs::write(&empty, b"   \n\n").expect("fixture must be written");
    let error = extract_document(&empty).expect_err("empty document must fail");
    assert_eq!(error.code(), "document_empty");
}

#[test]
fn rejects_files_larger_than_twenty_mebibytes_before_reading() {
    let directory = fixture_dir();
    let path = directory.path().join("large.md");
    let file = fs::File::create(&path).expect("fixture must be created");
    file.set_len(coldstart_adapters::MAX_DOCUMENT_BYTES + 1)
        .expect("fixture length must be set");

    let error = extract_document(&path).expect_err("oversized document must fail");
    assert!(matches!(error, DocumentError::TooLarge { .. }));
    assert_eq!(error.code(), "document_too_large");
}

fn build_pdf(content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}
