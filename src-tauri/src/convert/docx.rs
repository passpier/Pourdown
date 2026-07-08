use std::collections::HashMap;
use std::io::{Cursor, Read};

use docx_rs::{
    read_docx, DocumentChild, Docx, Drawing, DrawingData, HyperlinkData, Paragraph,
    ParagraphChild, Run, RunChild, StructuredDataTag, StructuredDataTagChild, Table,
    TableCellContent, TableChild, TableRowChild,
};

use super::media::MediaSink;
use super::ConversionError;

/// Resolved image relationships for a DOCX: rId → `word/media/...` archive
/// path, and the raw bytes of every `word/media/*` part.
struct DocxMedia {
    rels: HashMap<String, String>,
    parts: HashMap<String, Vec<u8>>,
}

/// Convert a DOCX file to Markdown text.
///
/// Known limitations (by design, not surfaced as errors):
/// - Track changes, comments, footnotes are dropped
/// - Complex layouts (text boxes, columns) may have scrambled order
/// - TOC fields are replaced with an HTML comment placeholder, but the TOC
///   entries themselves (rendered as paragraphs of internal-anchor
///   hyperlinks) are preserved as Markdown anchor links (`[text](#anchor)`),
///   matching MarkItDown's import output. The anchors won't resolve against
///   Markdown's auto-generated heading slugs, but are kept for fidelity.
/// - Vector image formats (EMF/WMF) can't render in the webview; a text note
///   is emitted in their place
pub fn docx_to_markdown(path: &str, media: &mut MediaSink) -> Result<String, ConversionError> {
    let bytes =
        std::fs::read(path).map_err(|e| ConversionError(format!("Failed to read file: {}", e)))?;

    let docx = read_docx(&bytes)
        .map_err(|e| ConversionError(format!("Failed to parse DOCX: {:?}", e)))?;

    // Build numId -> is_ordered map from the document's numbering definitions.
    let num_map = build_numbering_map(&docx);

    // Resolve embedded image relationships by reopening the raw ZIP.
    let docx_media = load_docx_media(&bytes);

    let mut output = String::new();
    let mut first_block = true;

    for child in &docx.document.children {
        match child {
            DocumentChild::Paragraph(para) => {
                let md = paragraph_to_markdown(para, &num_map, &docx_media, media);
                if md.trim().is_empty() {
                    if !first_block {
                        output.push('\n');
                    }
                } else {
                    if !first_block {
                        output.push('\n');
                    }
                    output.push_str(&md);
                    output.push('\n');
                    first_block = false;
                }
            }
            DocumentChild::Table(table) => {
                if !first_block {
                    output.push('\n');
                }
                output.push_str(&table_to_markdown(table, &num_map, &docx_media, media));
                output.push('\n');
                first_block = false;
            }
            DocumentChild::StructuredDataTag(sdt) => {
                let md = sdt_to_markdown(sdt, &num_map, &docx_media, media);
                if !md.trim().is_empty() {
                    if !first_block {
                        output.push('\n');
                    }
                    output.push_str(&md);
                    first_block = false;
                }
            }
            DocumentChild::TableOfContents(_) => {
                if !first_block {
                    output.push('\n');
                }
                output.push_str("<!-- Table of Contents omitted -->\n");
                first_block = false;
            }
            _ => {}
        }
    }

    Ok(output)
}

/// Reopen the raw DOCX ZIP to resolve `word/_rels/document.xml.rels`
/// (rId → media target) and read every `word/media/*` part's bytes.
/// docx-rs itself doesn't expose these OOXML parts.
fn load_docx_media(bytes: &[u8]) -> DocxMedia {
    let mut rels = HashMap::new();
    let mut parts = HashMap::new();

    if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) {
        for i in 0..archive.len() {
            if let Ok(mut entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if name == "word/_rels/document.xml.rels" {
                    let mut content = String::new();
                    if entry.read_to_string(&mut content).is_ok() {
                        rels = parse_document_rels(&content);
                    }
                } else if name.starts_with("word/media/") {
                    let mut buf = Vec::new();
                    if entry.read_to_end(&mut buf).is_ok() {
                        parts.insert(name, buf);
                    }
                }
            }
        }
    }

    DocxMedia { rels, parts }
}

/// Parse `word/_rels/document.xml.rels` into rId → resolved `word/media/...`
/// path. `Target` is relative to the `word/` directory (e.g. `media/image1.png`).
fn parse_document_rels(rels_xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for chunk in rels_xml.split("<Relationship ") {
        if !chunk.contains("/image") {
            continue;
        }
        if let (Some(id), Some(target)) = (get_rels_attr(chunk, "Id"), get_rels_attr(chunk, "Target")) {
            map.insert(id, resolve_word_relative_path(&target));
        }
    }
    map
}

fn get_rels_attr(s: &str, attr: &str) -> Option<String> {
    let search = format!("{}=\"", attr);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// Resolve a path relative to `word/` (e.g. `media/image1.png`) into an
/// absolute-in-archive path (e.g. `word/media/image1.png`).
fn resolve_word_relative_path(target: &str) -> String {
    let mut base: Vec<&str> = vec!["word"];
    for segment in target.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                base.pop();
            }
            other => base.push(other),
        }
    }
    base.join("/")
}

/// If `run` contains an embedded picture, extract it via `media` and return a
/// Markdown image link (or an "(unsupported image)" note for non-renderable
/// formats like EMF/WMF).
fn run_image_markdown(run: &Run, docx_media: &DocxMedia, media: &mut MediaSink) -> Option<String> {
    for child in &run.children {
        if let RunChild::Drawing(drawing) = child {
            if let Drawing {
                data: Some(DrawingData::Pic(pic)),
                ..
            } = drawing.as_ref()
            {
                let media_path = docx_media.rels.get(&pic.id)?;
                return Some(match docx_media.parts.get(media_path) {
                    Some(bytes) => match media.add(media_path, bytes) {
                        Some(rel_path) => format!("![]({})", rel_path),
                        None => format!(
                            "*(unsupported image: {})*",
                            media_path.rsplit('/').next().unwrap_or(media_path)
                        ),
                    },
                    None => format!(
                        "*(unsupported image: {})*",
                        media_path.rsplit('/').next().unwrap_or(media_path)
                    ),
                });
            }
        }
    }
    None
}

/// Build a lookup from numbering id → is_ordered (true = numbered list, false = bullet).
/// Resolves via abstract_num_id → level 0 format.
fn build_numbering_map(docx: &Docx) -> HashMap<usize, bool> {
    // abstract_num_id → is_ordered
    let abstract_map: HashMap<usize, bool> = docx
        .numberings
        .abstract_nums
        .iter()
        .map(|abs| {
            let ordered = abs
                .levels
                .first()
                .map(|l| is_ordered_format(&l.format.val))
                .unwrap_or(false);
            (abs.id, ordered)
        })
        .collect();

    docx.numberings
        .numberings
        .iter()
        .map(|n| {
            let ordered = abstract_map.get(&n.abstract_num_id).copied().unwrap_or(false);
            (n.id, ordered)
        })
        .collect()
}

fn is_ordered_format(val: &str) -> bool {
    matches!(
        val,
        "decimal"
            | "decimalZero"
            | "lowerLetter"
            | "upperLetter"
            | "lowerRoman"
            | "upperRoman"
            | "ordinal"
            | "cardinalText"
            | "ordinalText"
            | "decimalEnclosedCircle"
            | "decimalEnclosedFullstop"
            | "decimalEnclosedParen"
    )
}

fn paragraph_to_markdown(
    para: &Paragraph,
    num_map: &HashMap<usize, bool>,
    docx_media: &DocxMedia,
    media: &mut MediaSink,
) -> String {
    let style_id = para.property.style.as_ref().map(|s| s.val.to_lowercase());
    let style_str = style_id.as_deref().unwrap_or("");

    let heading_prefix: &str = match style_str {
        "heading1" | "heading 1" | "1" => "# ",
        "heading2" | "heading 2" | "2" => "## ",
        "heading3" | "heading 3" | "3" => "### ",
        "heading4" | "heading 4" | "4" => "#### ",
        "heading5" | "heading 5" | "5" => "##### ",
        "heading6" | "heading 6" | "6" => "###### ",
        "subtitle" => "## ",
        _ => "",
    };

    // "title" style renders as bold text, not as a heading level
    let is_title = style_str == "title";

    // Fallback: use outline level when the style ID isn't a known heading
    let heading_prefix = if heading_prefix.is_empty() && !is_title {
        para.property
            .outline_lvl
            .as_ref()
            .map(|o| match o.v {
                0 => "# ",
                1 => "## ",
                2 => "### ",
                3 => "#### ",
                4 => "##### ",
                5 => "###### ",
                _ => "", // levels 6–9 are body text in DOCX, not headings
            })
            .unwrap_or("")
    } else {
        heading_prefix
    };

    let list_prefix = if heading_prefix.is_empty() && !is_title {
        para.property.numbering_property.as_ref().map(|np| {
            let num_id = np.id.as_ref().map(|i| i.id).unwrap_or(0);
            let level = np.level.as_ref().map(|l| l.val).unwrap_or(0);
            let indent = "  ".repeat(level);
            let marker = if *num_map.get(&num_id).unwrap_or(&false) {
                "1. "
            } else {
                "- "
            };
            format!("{}{}", indent, marker)
        })
    } else {
        None
    };

    // Collect (text, bold, italic, strike) segments from all paragraph children
    let mut segments: Vec<(String, bool, bool, bool)> = Vec::new();

    for child in &para.children {
        match child {
            ParagraphChild::Run(run) => {
                if let Some(img_md) = run_image_markdown(run, docx_media, media) {
                    segments.push((img_md, false, false, false));
                } else if let Some(seg) = run_to_segment(run, is_title) {
                    segments.push(seg);
                }
            }
            ParagraphChild::Hyperlink(hyperlink) => {
                let mut inner = String::new();
                for c in &hyperlink.children {
                    if let ParagraphChild::Run(r) = c {
                        inner.push_str(&run_raw_text(r));
                    }
                }
                if !inner.is_empty() {
                    let linked = match &hyperlink.link {
                        HyperlinkData::External { path, .. } => {
                            format!("[{}]({})", inner, path)
                        }
                        // Internal anchor (e.g. TOC entries): keep as a Markdown
                        // anchor link to match MarkItDown's import output. The
                        // target won't resolve against Markdown's auto-generated
                        // heading slugs, but we preserve it for fidelity.
                        HyperlinkData::Anchor { anchor } if !anchor.is_empty() => {
                            format!("[{}](#{})", inner, anchor)
                        }
                        HyperlinkData::Anchor { .. } => inner, // empty anchor -> plain text
                    };
                    segments.push((linked, false, false, false));
                }
            }
            _ => {}
        }
    }

    // Merge adjacent segments with identical formatting to prevent `****` artifacts
    let mut merged: Vec<(String, bool, bool, bool)> = Vec::new();
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if last.1 == seg.1 && last.2 == seg.2 && last.3 == seg.3 {
                last.0.push_str(&seg.0);
                continue;
            }
        }
        merged.push(seg);
    }

    let mut text = String::new();
    for (t, bold, italic, strike) in merged {
        text.push_str(&apply_inline_fmt(&t, bold, italic, strike));
    }

    if text.is_empty() {
        return String::new();
    }

    let page_break = if para.property.page_break_before == Some(true) {
        "---\n\n"
    } else {
        ""
    };

    match list_prefix {
        Some(prefix) => format!("{}{}{}", page_break, prefix, text),
        None => format!("{}{}{}", page_break, heading_prefix, text),
    }
}

/// Extract raw text from a run (no markdown emphasis markers, but literal
/// Markdown-significant characters are escaped — see `escape_markdown`).
fn run_raw_text(run: &Run) -> String {
    let mut text = String::new();
    for child in &run.children {
        match child {
            RunChild::Text(t) => text.push_str(&escape_markdown(&t.text)),
            RunChild::Tab(_) => text.push(' '),
            _ => {}
        }
    }
    text
}

/// Escape literal Markdown inline-emphasis/code characters that appear in raw
/// Word text, so author-typed `*`, `_`, backticks aren't reinterpreted as
/// Markdown (e.g. a leading `*` becoming a bullet). Backslash is escaped first.
/// Block-level leading markers (- + # >) are intentionally NOT escaped (a
/// deliberate, narrower scope) to avoid mangling common text like dates
/// ("2024-07-26") and "Item #5".
fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Return a single (text, bold, italic, strike) segment for a run, or None if empty.
fn run_to_segment(run: &Run, force_bold: bool) -> Option<(String, bool, bool, bool)> {
    let text = run_raw_text(run);
    if text.is_empty() {
        return None;
    }
    let bold = force_bold || run.run_property.bold.is_some();
    let italic = run.run_property.italic.is_some();
    let strike = run.run_property.strike.as_ref().map(|s| s.val).unwrap_or(false)
        || run.run_property.dstrike.as_ref().map(|d| d.val).unwrap_or(false);
    Some((text, bold, italic, strike))
}

/// Apply bold/italic/strikethrough markers, skipping whitespace-only text.
///
/// CommonMark requires an emphasis opener/closer to hug its text — `**`
/// immediately followed by whitespace is not a valid opener, so a run like
/// `"  Title"` wrapped naively as `"**  Title**"` renders as literal
/// asterisks. Leading/trailing whitespace is moved outside the markers so
/// emphasis stays valid while inter-run spacing (e.g. between adjacent runs
/// in the same paragraph) is preserved.
fn apply_inline_fmt(text: &str, bold: bool, italic: bool, strike: bool) -> String {
    if text.trim().is_empty() || (!bold && !italic && !strike) {
        return text.to_string();
    }
    let trimmed_start = text.trim_start();
    let leading = &text[..text.len() - trimmed_start.len()];
    let core = trimmed_start.trim_end();
    let trailing = &trimmed_start[core.len()..];

    let s = if strike {
        format!("~~{}~~", core)
    } else {
        core.to_string()
    };
    let wrapped = match (bold, italic) {
        (true, true) => format!("***{}***", s),
        (true, false) => format!("**{}**", s),
        (false, true) => format!("*{}*", s),
        (false, false) => s,
    };
    format!("{}{}{}", leading, wrapped, trailing)
}

fn run_to_markdown(run: &Run, docx_media: &DocxMedia, media: &mut MediaSink) -> String {
    if let Some(img_md) = run_image_markdown(run, docx_media, media) {
        return img_md;
    }
    match run_to_segment(run, false) {
        None => String::new(),
        Some((text, bold, italic, strike)) => apply_inline_fmt(&text, bold, italic, strike),
    }
}

fn sdt_to_markdown(
    sdt: &StructuredDataTag,
    num_map: &HashMap<usize, bool>,
    docx_media: &DocxMedia,
    media: &mut MediaSink,
) -> String {
    let mut output = String::new();
    for child in &sdt.children {
        match child {
            StructuredDataTagChild::Paragraph(para) => {
                let md = paragraph_to_markdown(para, num_map, docx_media, media);
                if !md.trim().is_empty() {
                    output.push_str(&md);
                    output.push('\n');
                }
            }
            StructuredDataTagChild::Table(table) => {
                output.push_str(&table_to_markdown(table, num_map, docx_media, media));
            }
            StructuredDataTagChild::Run(run) => {
                let md = run_to_markdown(run, docx_media, media);
                if !md.is_empty() {
                    output.push_str(&md);
                }
            }
            StructuredDataTagChild::StructuredDataTag(nested) => {
                let md = sdt_to_markdown(nested, num_map, docx_media, media);
                if !md.is_empty() {
                    output.push_str(&md);
                }
            }
            _ => {}
        }
    }
    output
}

fn table_to_markdown(
    table: &Table,
    num_map: &HashMap<usize, bool>,
    docx_media: &DocxMedia,
    media: &mut MediaSink,
) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();

    for row_child in &table.rows {
        let TableChild::TableRow(table_row) = row_child;
        let mut cells: Vec<String> = Vec::new();
        for cell_child in &table_row.cells {
            let TableRowChild::TableCell(table_cell) = cell_child;
            let mut cell_text = String::new();
            for content in &table_cell.children {
                if let TableCellContent::Paragraph(para) = content {
                    let p = paragraph_to_markdown(para, num_map, docx_media, media);
                    if !p.is_empty() {
                        if !cell_text.is_empty() {
                            cell_text.push(' ');
                        }
                        cell_text.push_str(p.trim());
                    }
                }
            }
            cells.push(cell_text);
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return String::new();
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return String::new();
    }

    let mut md = String::new();

    // Header row
    let header = &rows[0];
    md.push('|');
    for i in 0..col_count {
        let cell = header.get(i).map(|s| s.as_str()).unwrap_or("");
        md.push_str(&format!(" {} |", cell));
    }
    md.push('\n');

    // Separator
    md.push('|');
    for _ in 0..col_count {
        md.push_str(" --- |");
    }
    md.push('\n');

    // Data rows
    for row in rows.iter().skip(1) {
        md.push('|');
        for i in 0..col_count {
            let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
            md.push_str(&format!(" {} |", cell));
        }
        md.push('\n');
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_media() -> DocxMedia {
        DocxMedia {
            rels: HashMap::new(),
            parts: HashMap::new(),
        }
    }

    #[test]
    fn test_run_to_markdown_bold() {
        let run = Run::new().add_text("hello").bold();
        let mut sink = MediaSink::new(std::env::temp_dir());
        let result = run_to_markdown(&run, &empty_media(), &mut sink);
        assert_eq!(result, "**hello**");
    }

    #[test]
    fn test_run_to_markdown_plain() {
        let run = Run::new().add_text("hello");
        let mut sink = MediaSink::new(std::env::temp_dir());
        let result = run_to_markdown(&run, &empty_media(), &mut sink);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_paragraph_anchor_hyperlink_becomes_link() {
        use docx_rs::{Hyperlink, HyperlinkType};
        let para = Paragraph::new().add_hyperlink(
            Hyperlink::new("_Toc181806136", HyperlinkType::Anchor)
                .add_run(Run::new().add_text("TABLE OF CONTENTS 1")),
        );
        let mut sink = MediaSink::new(std::env::temp_dir());
        let md = paragraph_to_markdown(&para, &HashMap::new(), &empty_media(), &mut sink);
        assert_eq!(md, "[TABLE OF CONTENTS 1](#_Toc181806136)");
    }

    #[test]
    fn test_run_to_markdown_whitespace_bold_not_wrapped() {
        let run = Run::new().add_text("   ").bold();
        let mut sink = MediaSink::new(std::env::temp_dir());
        let result = run_to_markdown(&run, &empty_media(), &mut sink);
        assert_eq!(result, "   ");
    }

    #[test]
    fn test_apply_inline_fmt_moves_leading_whitespace_outside_markers() {
        // `**  Foo**` is not valid CommonMark bold (opener can't be followed by
        // whitespace); the space must move outside the markers.
        assert_eq!(
            apply_inline_fmt("  Foo (FSD)", true, false, false),
            "  **Foo (FSD)**"
        );
        assert_eq!(
            apply_inline_fmt(" Version : 1.0", true, false, false),
            " **Version : 1.0**"
        );
    }

    #[test]
    fn test_apply_inline_fmt_moves_trailing_whitespace_outside_markers() {
        assert_eq!(apply_inline_fmt("bold ", true, false, false), "**bold** ");
    }

    #[test]
    fn test_escape_markdown_literal_asterisk() {
        // A literal `*` at the start of list item text (e.g. Word's own
        // "* means mandatory field" convention) must not be read as a bullet.
        assert_eq!(escape_markdown("* means mandatory"), "\\* means mandatory");
        assert_eq!(escape_markdown("a_b*c`d"), "a\\_b\\*c\\`d");
        assert_eq!(escape_markdown("hello world"), "hello world");
    }

    #[test]
    fn test_run_to_markdown_bold_with_leading_whitespace_and_literal_asterisk() {
        let run = Run::new().add_text("  * means mandatory").bold();
        let mut sink = MediaSink::new(std::env::temp_dir());
        let result = run_to_markdown(&run, &empty_media(), &mut sink);
        assert_eq!(result, "  **\\* means mandatory**");
    }

    /// End-to-end regression test against `tests/fixtures/sample.docx`
    /// (see `src/fixture_gen.rs` for how it's generated). Covers heading
    /// detection, bold/italic/strike, bullet + numbered lists, tables, and
    /// embedded images together, since those are the documented per-format
    /// behaviors in markdown-import.md.
    #[test]
    fn test_docx_to_markdown_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.docx");
        let dir = std::env::temp_dir().join(format!("pourdown-docx-fixture-{}", std::process::id()));
        let mut sink = MediaSink::new(dir.clone());

        let md = docx_to_markdown(path, &mut sink).expect("docx_to_markdown should succeed");

        assert!(md.contains("# Sample Heading"), "heading not detected:\n{md}");
        assert!(md.contains("**bold**"), "bold run not detected:\n{md}");
        assert!(md.contains("*italic*"), "italic run not detected:\n{md}");
        assert!(md.contains("~~struck~~"), "strike run not detected:\n{md}");
        assert!(md.contains("- First bullet"), "bullet list not detected:\n{md}");
        assert!(md.contains("1. First step"), "numbered list not detected:\n{md}");
        assert!(md.contains("| Name |") && md.contains("| Ada |"), "table not detected:\n{md}");
        assert!(md.contains("![](assets/image1.png)"), "image link not detected:\n{md}");
        // MediaSink's assets_dir *is* the assets folder (see `assets_dir` in
        // main.rs), so the file lands directly under `dir`, not `dir/assets`.
        assert!(dir.join("image1.png").exists(), "image sidecar file not written");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
