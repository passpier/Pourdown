//! Regenerates the binary fixtures under `tests/fixtures/` used by the
//! converter tests in `src/convert/{docx,xlsx,pptx,pdf}.rs`.
//!
//! This is test-only code (never built into the app) and is not run as part
//! of the default `cargo test` — it's gated `#[ignore]` since it overwrites
//! committed fixtures. Regenerate explicitly after changing this file:
//!
//!   cargo test regenerate_fixtures -- --ignored --nocapture
//!
//! Each fixture is deliberately minimal: it's built to exercise the specific
//! documented behaviors covered by the converter tests (see markdown-import.md
//! and CLAUDE.md), not to be a full-fidelity real-world sample of the format.
//! The xlsx/pptx fixtures are hand-assembled OOXML (only the archive parts
//! Pourdown's own reader actually consumes) rather than produced by an
//! Office-compatible writer library, since none is a project dependency.

use std::io::Write;
use std::path::{Path, PathBuf};

use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, NumberFormat, Numbering,
    NumberingId, Paragraph, Pic, Run, Start, Table, TableCell, TableRow,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A tiny solid-color PNG, generated in-memory (no binary blob checked in
/// separately from this generator).
fn tiny_png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 30]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("encode tiny PNG");
    buf
}

#[test]
#[ignore = "regenerates tests/fixtures/*; run explicitly, not part of the default suite"]
fn regenerate_fixtures() {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("create tests/fixtures");
    write_sample_docx(&dir.join("sample.docx"));
    write_sample_xlsx(&dir.join("sample.xlsx"));
    write_sample_pptx(&dir.join("sample.pptx"));
    write_sample_pdf(&dir.join("sample.pdf"));
}

/// An H1 heading, bold/italic/strike runs, a bullet list, a numbered list, a
/// two-column table, and one embedded image — covers docx_to_markdown's
/// documented per-format behaviors.
fn write_sample_docx(path: &Path) {
    let bullet_abstract = AbstractNumbering::new(1).add_level(Level::new(
        0,
        Start::new(1),
        NumberFormat::new("bullet"),
        LevelText::new("\u{2022}"),
        LevelJc::new("left"),
    ));
    let decimal_abstract = AbstractNumbering::new(2).add_level(Level::new(
        0,
        Start::new(1),
        NumberFormat::new("decimal"),
        LevelText::new("%1."),
        LevelJc::new("left"),
    ));

    let docx = Docx::new()
        .add_abstract_numbering(bullet_abstract)
        .add_abstract_numbering(decimal_abstract)
        .add_numbering(Numbering::new(1, 1))
        .add_numbering(Numbering::new(2, 2))
        .add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("Sample Heading"))
                .style("Heading1"),
        )
        .add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("This is "))
                .add_run(Run::new().add_text("bold").bold())
                .add_run(Run::new().add_text(", "))
                .add_run(Run::new().add_text("italic").italic())
                .add_run(Run::new().add_text(", and "))
                .add_run(Run::new().add_text("struck").strike())
                .add_run(Run::new().add_text(" text.")),
        )
        .add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("First bullet"))
                .numbering(NumberingId::new(1), IndentLevel::new(0)),
        )
        .add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("Second bullet"))
                .numbering(NumberingId::new(1), IndentLevel::new(0)),
        )
        .add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("First step"))
                .numbering(NumberingId::new(2), IndentLevel::new(0)),
        )
        .add_table(Table::new(vec![
            TableRow::new(vec![
                TableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Name"))),
                TableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Role"))),
            ]),
            TableRow::new(vec![
                TableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Ada"))),
                TableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Engineer"))),
            ]),
        ]))
        .add_paragraph(Paragraph::new().add_run(Run::new().add_image(Pic::new(&tiny_png()))));

    let file = std::fs::File::create(path).expect("create sample.docx");
    docx.build().pack(file).expect("pack sample.docx");
}

/// Two sheets: "Data" has a header-detected date column (Excel serial) and a
/// multi-line cell (collapsed to one line by `cell_to_string`); "Notes" is a
/// minimal second sheet. Assembled as raw OOXML (calamine has no writer
/// counterpart) using inline strings, so no `sharedStrings.xml` part is
/// needed.
fn write_sample_xlsx(path: &Path) {
    const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#;

    const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;

    const WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>
<sheet name="Data" sheetId="1" r:id="rId1"/>
<sheet name="Notes" sheetId="2" r:id="rId2"/>
</sheets>
</workbook>"#;

    const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
</Relationships>"#;

    // B2/B3 hold the Excel serial 45000/45100 — the "Date" header makes
    // xlsx_to_markdown reinterpret them as ISO dates. C2 has an embedded
    // newline to exercise the single-line collapse in cell_to_string.
    const SHEET1: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1">
<c r="A1" t="inlineStr"><is><t>Name</t></is></c>
<c r="B1" t="inlineStr"><is><t>Date</t></is></c>
<c r="C1" t="inlineStr"><is><t>Notes</t></is></c>
</row>
<row r="2">
<c r="A2" t="inlineStr"><is><t>Alice</t></is></c>
<c r="B2"><v>45000</v></c>
<c r="C2" t="inlineStr"><is><t>Line one
Line two</t></is></c>
</row>
<row r="3">
<c r="A3" t="inlineStr"><is><t>Bob</t></is></c>
<c r="B3"><v>45100</v></c>
<c r="C3" t="inlineStr"><is><t>Done</t></is></c>
</row>
</sheetData>
</worksheet>"#;

    const SHEET2: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1">
<c r="A1" t="inlineStr"><is><t>Comment</t></is></c>
</row>
<row r="2">
<c r="A2" t="inlineStr"><is><t>Second sheet for smoke coverage</t></is></c>
</row>
</sheetData>
</worksheet>"#;

    let file = std::fs::File::create(path).expect("create sample.xlsx");
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default();
    for (name, content) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("xl/workbook.xml", WORKBOOK),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS),
        ("xl/worksheets/sheet1.xml", SHEET1),
        ("xl/worksheets/sheet2.xml", SHEET2),
    ] {
        zip.start_file(name, opts).unwrap_or_else(|e| panic!("start_file {name}: {e}"));
        zip.write_all(content.as_bytes()).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }
    zip.finish().expect("finish sample.xlsx");
}

/// Two slides separated by `---`: slide 1 has a title placeholder and a
/// non-bulleted bold paragraph; slide 2 has a title, a bulleted paragraph,
/// and an embedded image. Only the archive parts pptx_to_markdown actually
/// reads are included (no `presentation.xml` — Pourdown's reader never opens
/// it, so a real PowerPoint-openable file isn't required for this fixture).
fn write_sample_pptx(path: &Path) {
    const SLIDE1: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:r><a:t>Slide One</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:txBody>
<a:p><a:pPr><a:buNone/></a:pPr><a:r><a:rPr b="1"/><a:t>Bold intro</a:t></a:r></a:p>
</p:txBody></p:sp>
</p:spTree></p:cSld>
</p:sld>"#;

    const SLIDE2: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:r><a:t>Slide Two</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:txBody>
<a:p><a:r><a:t>First bullet</a:t></a:r></a:p>
</p:txBody></p:sp>
<p:pic><p:blipFill><a:blip r:embed="rId1"/></p:blipFill></p:pic>
</p:spTree></p:cSld>
</p:sld>"#;

    const SLIDE2_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
</Relationships>"#;

    let file = std::fs::File::create(path).expect("create sample.pptx");
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    let entries: [(&str, &[u8]); 4] = [
        ("ppt/slides/slide1.xml", SLIDE1.as_bytes()),
        ("ppt/slides/slide2.xml", SLIDE2.as_bytes()),
        ("ppt/slides/_rels/slide2.xml.rels", SLIDE2_RELS.as_bytes()),
        ("ppt/media/image1.png", &tiny_png()),
    ];
    for (name, bytes) in entries {
        zip.start_file(name, opts).unwrap_or_else(|e| panic!("start_file {name}: {e}"));
        zip.write_all(bytes).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }
    zip.finish().expect("finish sample.pptx");
}

/// Generated via Pourdown's own `markdown_to_pdf` export rather than a
/// hand-authored PDF byte stream, so the fixture is guaranteed pdfium-valid
/// without hand-rolling an xref table. Includes a short ALL-CAPS line to
/// exercise the `is_all_caps_heading` fallback path.
fn write_sample_pdf(path: &Path) {
    let markdown = "# SAMPLE REPORT\n\n\
        This paragraph should survive the PDF roundtrip.\n\n\
        OVERVIEW\n\n\
        Second paragraph after the all-caps heading line.\n";
    crate::convert::pdf::markdown_to_pdf(markdown, path.to_str().expect("utf8 path"))
        .expect("markdown_to_pdf should succeed");
}
