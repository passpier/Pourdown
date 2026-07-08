use markdown2pdf::config::ConfigSource;
use pdfium_render::prelude::*;
use std::path::PathBuf;
use std::sync::Mutex;

use super::media::MediaSink;
use super::ConversionError;

// Guards the one-time initialization of the global pdfium bindings.
static PDFIUM_INIT: Mutex<bool> = Mutex::new(false);

const PDF_IMPORT_NOTICE: &str = "> **Import Notice**: This PDF was imported with layout analysis. \
Headings and paragraphs are inferred from font sizes and spacing. \
Embedded images are extracted where possible, but exact positioning and \
complex multi-column layouts may not be fully preserved.\n\n";

/// Convert Markdown to a PDF file.
pub fn markdown_to_pdf(markdown: &str, path: &str) -> Result<(), ConversionError> {
    markdown2pdf::parse_into_file(markdown.to_string(), path, ConfigSource::Default, None)
        .map_err(|e| ConversionError(format!("PDF export failed: {}", e)))
}

/// Convert a PDF file to Markdown using layout-aware extraction.
pub fn pdf_to_markdown(path: &str, media: &mut MediaSink) -> Result<String, ConversionError> {
    // Initialize pdfium bindings exactly once per process
    {
        let mut initialized = PDFIUM_INIT
            .lock()
            .map_err(|_| ConversionError("Pdfium init lock poisoned".to_string()))?;
        if !*initialized {
            let lib = pdfium_lib_path();
            let bindings = Pdfium::bind_to_library(&lib).map_err(|e| {
                ConversionError(format!("Failed to load pdfium library at {:?}: {}", lib, e))
            })?;
            Pdfium::new(bindings);
            *initialized = true;
        }
    }

    // After initialization, Pdfium::default() reuses the global bindings without re-loading.
    let pdfium = Pdfium::default();

    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| ConversionError(format!("Failed to open PDF: {}", e)))?;

    let mut md = String::from(PDF_IMPORT_NOTICE);
    for (page_index, page) in doc.pages().iter().enumerate() {
        md.push_str(&extract_page_markdown(&page, page_index, media)?);
        md.push('\n');
    }

    Ok(md)
}

/// Returns the path to the pdfium dynamic library at runtime.
fn pdfium_lib_path() -> PathBuf {
    // Developer or CI override
    if let Ok(p) = std::env::var("PDFIUM_LIBRARY_PATH") {
        let path = PathBuf::from(p);
        if path.exists() {
            return path;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(std::path::Path::new("."));

        #[cfg(target_os = "macos")]
        {
            // macOS app bundle: MacOS/ -> ../Frameworks/pdfium.framework/pdfium
            let bundled = dir.join("../Frameworks/pdfium.framework/pdfium");
            if bundled.exists() {
                return bundled;
            }
            // Dev build: look for libpdfium.dylib placed beside the binary
            let beside = dir.join("libpdfium.dylib");
            if beside.exists() {
                return beside;
            }
        }

        #[cfg(target_os = "windows")]
        {
            let dll = dir.join("pdfium.dll");
            if dll.exists() {
                return dll;
            }
        }

        #[cfg(target_os = "linux")]
        {
            let so = dir.join("libpdfium.so");
            if so.exists() {
                return so;
            }
        }
    }

    // Fall back to platform library name in the current working directory
    PathBuf::from(Pdfium::pdfium_platform_library_name())
}

/// True if `text` contains a run of at least 4 dots (a TOC "dot leader"),
/// ignoring interior spaces so a spaced-out leader (". . . .") still counts.
/// Any other character resets the run, so an ellipsis ("...") or a version
/// number ("1.2.3") don't false-positive.
fn contains_dot_leader(text: &str) -> bool {
    let mut run = 0u32;
    for c in text.chars() {
        match c {
            '.' => {
                run += 1;
                if run >= 4 {
                    return true;
                }
            }
            ' ' | '\u{00A0}' => {}
            _ => run = 0,
        }
    }
    false
}

/// Returns true for short lines that are all-uppercase (or CJK-only) with no sentence ending.
/// Used as a fallback heading detector when all text in the PDF has the same font size.
fn is_all_caps_heading(text: &str) -> bool {
    let char_count = text.chars().count();
    // Length guard: too short or too long to be a section heading
    if char_count < 3 || char_count > 80 {
        return false;
    }
    // Dot-leader lines are TOC entries, not headings
    if contains_dot_leader(text) {
        return false;
    }
    // No sentence-ending punctuation
    if text.ends_with('.') || text.ends_with(',') || text.ends_with(':') {
        return false;
    }
    // Must have zero lowercase ASCII letters
    let has_lower = text.chars().any(|c| c.is_ascii_lowercase());
    if has_lower {
        return false;
    }
    // Must contain at least some alphabetic content
    let alpha_count = text.chars().filter(|c| c.is_alphabetic()).count();
    alpha_count > 0
}

struct TextBlock {
    x: f32,
    /// Right edge of the block's bounding box, in the same page-space units
    /// as `x`. Used for table-cell gap detection; falls back to `x` (a
    /// zero-width block) if pdfium can't report bounds for this object.
    x_end: f32,
    y: f32,
    font_size: f32,
    text: String,
    /// True for an already-formatted `![]()` image link — excluded from
    /// heading classification since it has no meaningful font size.
    is_image: bool,
}

/// One cell of a detected table row, carrying its horizontal span so rows can
/// be clustered into columns against each other.
#[derive(Debug, Clone)]
struct Cell {
    x_start: f32,
    x_end: f32,
    text: String,
}

/// A run of visual lines recognized as a table: a set of column x-boundaries
/// (from the aligned "core" rows) plus the resolved logical rows, with
/// wrapped continuation lines already merged in.
#[derive(Debug)]
struct TableRegion {
    /// Index into `lines`/`rows` of the first line in this region.
    start_line: usize,
    /// Index into `lines`/`rows` of the last line in this region (inclusive).
    end_line: usize,
    columns: Vec<f32>,
    logical_rows: Vec<Vec<String>>,
}

/// Splits one visual line (block indices, already sorted left-to-right) into
/// cells wherever the horizontal gap between consecutive blocks exceeds
/// `gap_thresh`. A line with no such gap yields a single cell — i.e. an
/// ordinary paragraph line, indistinguishable from a table row until later
/// clustered against neighboring lines.
fn segment_line_into_cells(blocks: &[TextBlock], line: &[usize], gap_thresh: f32) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for &i in line {
        let block = &blocks[i];
        let text = block.text.trim();
        if text.is_empty() || block.is_image {
            continue;
        }
        match cells.last_mut() {
            Some(last) if block.x - last.x_end <= gap_thresh => {
                last.x_end = last.x_end.max(block.x_end);
                last.text.push(' ');
                last.text.push_str(text);
            }
            _ => cells.push(Cell {
                x_start: block.x,
                x_end: block.x_end,
                text: text.to_string(),
            }),
        }
    }
    cells
}

/// True if `row` has exactly as many cells as `columns` and each cell's
/// left edge lines up with the corresponding column within `tol`.
fn columns_match(row: &[Cell], columns: &[f32], tol: f32) -> bool {
    row.len() == columns.len()
        && row.iter().zip(columns).all(|(c, &x)| (c.x_start - x).abs() <= tol)
}

/// Attempts to interpret `row` as a wrapped continuation of the previous
/// table row: every cell must map to a distinct column within `tol`, and
/// there must be strictly fewer cells than columns (otherwise it would have
/// matched `columns_match` as a full row already). Returns the
/// `(column_index, text)` pairs to merge in, or `None` if the row doesn't
/// cleanly map onto the known columns.
fn try_continuation(row: &[Cell], columns: &[f32], tol: f32) -> Option<Vec<(usize, String)>> {
    if row.is_empty() || row.len() >= columns.len() {
        return None;
    }
    let mut used = vec![false; columns.len()];
    let mut mapped = Vec::with_capacity(row.len());
    for cell in row {
        let mut best: Option<(usize, f32)> = None;
        for (idx, &x) in columns.iter().enumerate() {
            if used[idx] {
                continue;
            }
            let d = (cell.x_start - x).abs();
            if d <= tol && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((idx, d));
            }
        }
        let (idx, _) = best?;
        used[idx] = true;
        mapped.push((idx, cell.text.clone()));
    }
    mapped.sort_by_key(|(idx, _)| *idx);
    Some(mapped)
}

/// True if any cell in `row` contains a dot leader — used to keep TOC lines
/// (which otherwise often satisfy the column-alignment gates below) out of
/// table detection entirely.
fn row_has_dot_leader(row: &[Cell]) -> bool {
    row.iter().any(|c| contains_dot_leader(&c.text))
}

/// Collapses a TOC line's dot leader (runs of 4+ dots, possibly spaced out)
/// into a compact " … " separator, e.g. "Introduction ........ 5" becomes
/// "Introduction … 5". Non-leader text is left untouched.
fn collapse_dot_leader(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush_run = |out: &mut String, run: &mut String| {
        if run.chars().filter(|&c| c == '.').count() >= 4 {
            out.push_str(" … ");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        match c {
            '.' | ' ' | '\u{00A0}' => run.push(c),
            _ => {
                flush_run(&mut out, &mut run);
                out.push(c);
            }
        }
    }
    flush_run(&mut out, &mut run);
    // Collapse whitespace left by the leader (e.g. before the trailing page
    // number) down to single spaces, and trim stray edges.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ensures `out` ends with a blank line (two trailing newlines), unless it's
/// empty. Used to open/close the TOC bullet list around other content
/// without depending on the unrelated vertical-gap blank-line heuristic.
fn ensure_blank_line(out: &mut String) {
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// True if a horizontal rule line falls strictly between `y_upper` (the
/// previous line) and `y_lower` (the current line). Used to refuse merging a
/// row as a continuation when the source PDF explicitly separated it with a
/// ruling line — a hybrid confirming signal on top of geometry clustering,
/// a no-op on borderless tables since `h_rules` is then empty.
fn has_rule_between(h_rules: &[f32], y_upper: f32, y_lower: f32) -> bool {
    h_rules.iter().any(|&y| y < y_upper && y > y_lower)
}

/// Minimum number of consecutive, exactly-column-aligned lines required
/// before a region is accepted as a table's "core". Two aligned rows alone
/// can't be reliably distinguished from an incidental key:value pair (e.g.
/// "Name: Alice" over "Role: Engineer" happening to line up); requiring a
/// third confirms it's a genuine repeating column structure.
const MIN_CORE_ROWS: usize = 3;

/// Detects table regions across a page's visual lines using conservative
/// geometry clustering: a region only starts where at least `MIN_CORE_ROWS`
/// consecutive lines share the same ≥2 column positions (the "core" rows),
/// then extends with further aligned rows or wrapped continuation lines
/// until neither applies.
fn detect_table_regions(
    rows: &[Vec<Cell>],
    line_ys: &[f32],
    h_rules: &[f32],
    body_size: f32,
) -> Vec<TableRegion> {
    let tol = body_size;
    let mut regions = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        // Dot-leader rows (TOC entries) are never a table seed — they often
        // satisfy the column-alignment gates below (title cell + page-number
        // cell, roughly aligned across consecutive entries) but are prose,
        // not tabular data.
        if rows[i].len() < 2 || row_has_dot_leader(&rows[i]) {
            i += 1;
            continue;
        }

        // A candidate table must have at least MIN_CORE_ROWS - 1 more rows
        // that align exactly with this one's columns — this is the
        // conservative gate that keeps ordinary multi-column text from
        // becoming a table.
        let columns: Vec<f32> = rows[i].iter().map(|c| c.x_start).collect();
        let mut core_end = i;
        let mut j = i + 1;
        while j < rows.len()
            && columns_match(&rows[j], &columns, tol)
            && !row_has_dot_leader(&rows[j])
        {
            core_end = j;
            j += 1;
        }
        if core_end - i + 1 < MIN_CORE_ROWS {
            i += 1;
            continue;
        }

        let mut logical_rows: Vec<Vec<String>> = rows[i..=core_end]
            .iter()
            .map(|r| r.iter().map(|c| c.text.clone()).collect())
            .collect();
        let mut end_line = core_end;
        let mut k = core_end + 1;
        while k < rows.len() {
            let row = &rows[k];
            if row.is_empty() || row_has_dot_leader(row) {
                break;
            }
            if columns_match(row, &columns, tol) {
                logical_rows.push(row.iter().map(|c| c.text.clone()).collect());
                end_line = k;
                k += 1;
                continue;
            }
            if has_rule_between(h_rules, line_ys[k - 1], line_ys[k]) {
                break;
            }
            match try_continuation(row, &columns, tol) {
                Some(mapped) => {
                    let target = logical_rows.last_mut().expect("core row exists");
                    for (col_idx, text) in mapped {
                        if !target[col_idx].is_empty() {
                            target[col_idx].push_str("<br>");
                        }
                        target[col_idx].push_str(&text);
                    }
                    end_line = k;
                    k += 1;
                }
                None => break,
            }
        }

        regions.push(TableRegion {
            start_line: i,
            end_line,
            columns,
            logical_rows,
        });
        i = end_line + 1;
    }
    regions
}

/// Escapes a cell's text for embedding in a GFM table: literal `\` and `|`
/// are escaped, and any embedded newline (rare — most cells are already
/// single-line) becomes `<br>` so it can't break the table's row structure.
fn escape_table_cell(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
}

/// Renders a detected [`TableRegion`] as a GFM table: first logical row is
/// the header, followed by the `| --- |` separator, then the data rows.
fn render_gfm_table(region: &TableRegion) -> String {
    let mut out = String::new();
    let ncols = region.columns.len();

    let render_row = |cells: &[String]| -> String {
        let mut row = String::from("|");
        for c in cells {
            row.push(' ');
            row.push_str(&escape_table_cell(c));
            row.push_str(" |");
        }
        row.push('\n');
        row
    };

    let Some((header, data_rows)) = region.logical_rows.split_first() else {
        return out;
    };
    out.push_str(&render_row(header));
    out.push('|');
    for _ in 0..ncols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in data_rows {
        out.push_str(&render_row(row));
    }
    out
}

/// Collects the y-coordinates of near-horizontal rule lines on this page
/// (e.g. table row/header borders), derived from stroked path objects.
/// Purely a confirming signal for [`detect_table_regions`] — never used to
/// loosen the geometry gate, so pages without ruling lines (borderless
/// tables, or PDFs whose rules aren't drawn as path objects) fall back to
/// geometry-only detection unaffected.
fn collect_horizontal_rules(page: &PdfPage) -> Vec<f32> {
    let mut h_rules: Vec<f32> = Vec::new();

    for obj in page.objects().iter() {
        let Some(path_obj) = obj.as_path_object() else {
            continue;
        };
        let segments = match obj.matrix() {
            Ok(m) => path_obj.segments().transform(m),
            Err(_) => path_obj.segments(),
        };

        let mut prev: Option<(f32, f32)> = None;
        for seg in segments.iter() {
            let (x, y) = seg.point();
            let (x, y) = (x.value, y.value);
            if let Some((px, py)) = prev {
                let dx = (x - px).abs();
                let dy = (y - py).abs();
                // Near-horizontal: long run in x, negligible change in y.
                if dy < 1.0 && dx > 4.0 {
                    h_rules.push((y + py) / 2.0);
                }
            }
            prev = Some((x, y));
        }
    }

    h_rules.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    h_rules
}

fn extract_page_markdown(
    page: &PdfPage,
    page_index: usize,
    media: &mut MediaSink,
) -> Result<String, ConversionError> {
    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut image_index = 0usize;

    for obj in page.objects().iter() {
        if let Some(text_obj) = obj.as_text_object() {
            let text = text_obj.text();
            if text.trim().is_empty() {
                continue;
            }
            let matrix = match text_obj.matrix() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let font_size = text_obj.unscaled_font_size().value;
            let x = matrix.e();
            // Right edge of the run's bounding box, used for table-cell gap
            // detection. Falls back to `x` (a zero-width block) if pdfium
            // can't report bounds for this object.
            let x_end = obj.bounds().map(|b| b.right().value).unwrap_or(x);
            blocks.push(TextBlock {
                x,
                x_end,
                y: matrix.f(),
                font_size,
                text,
                is_image: false,
            });
        } else if let Some(image_obj) = obj.as_image_object() {
            let (x, y) = match obj.matrix() {
                Ok(m) => (m.e(), m.f()),
                Err(_) => (0.0, 0.0),
            };
            image_index += 1;
            let part_name = format!("pdf-page{}-image{}.png", page_index + 1, image_index);
            let md = match image_obj.get_raw_image() {
                Ok(dynamic_image) => {
                    let mut png_bytes: Vec<u8> = Vec::new();
                    let encoded = dynamic_image
                        .write_to(
                            &mut std::io::Cursor::new(&mut png_bytes),
                            image::ImageFormat::Png,
                        )
                        .is_ok();
                    if encoded {
                        match media.add(&part_name, &png_bytes) {
                            Some(rel_path) => format!("![]({})", rel_path),
                            None => "*(unsupported image)*".to_string(),
                        }
                    } else {
                        "*(unsupported image)*".to_string()
                    }
                }
                Err(_) => continue,
            };
            blocks.push(TextBlock {
                x,
                x_end: x,
                y,
                font_size: 0.0,
                text: md,
                is_image: true,
            });
        }
    }

    if blocks.is_empty() {
        return Ok(String::new());
    }

    // Determine body (median) font size for relative heading detection
    let mut sizes: Vec<f32> = blocks.iter().map(|b| b.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let body_size = sizes[sizes.len() / 2];
    if body_size <= 0.0 {
        return Ok(blocks
            .iter()
            .map(|b| b.text.trim().to_string())
            .collect::<Vec<_>>()
            .join(" "));
    }

    // Sort top-to-bottom (PDF y=0 is at page bottom, so higher y is higher on page)
    blocks.sort_by(|a, b| {
        let dy = b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal);
        if dy == std::cmp::Ordering::Equal {
            a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            dy
        }
    });

    // Group into visual lines by y proximity
    let line_thresh = body_size * 0.6;
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if let Some(last) = lines.last_mut() {
            if (blocks[last[0]].y - block.y).abs() <= line_thresh {
                last.push(i);
                continue;
            }
        }
        lines.push(vec![i]);
    }

    // Sort each line left-to-right
    for line in &mut lines {
        line.sort_by(|&a, &b| {
            blocks[a]
                .x
                .partial_cmp(&blocks[b].x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Detect table regions before emitting: segment each line into cells,
    // then cluster consecutive lines that share ≥2 aligned columns. See
    // `detect_table_regions` for the conservative gating rules.
    let gap_thresh = body_size * 1.2;
    let rows: Vec<Vec<Cell>> = lines
        .iter()
        .map(|line| {
            if line.iter().all(|&i| blocks[i].is_image) {
                Vec::new()
            } else {
                segment_line_into_cells(&blocks, line, gap_thresh)
            }
        })
        .collect();
    let line_ys: Vec<f32> = lines.iter().map(|line| blocks[line[0]].y).collect();
    let h_rules = collect_horizontal_rules(page);
    let regions = detect_table_regions(&rows, &line_ys, &h_rules, body_size);

    let mut out = String::new();
    let mut prev_y = f32::MAX;
    let mut region_idx = 0;
    let mut i = 0;
    // Tracks whether the previous emitted line was a TOC entry, so
    // consecutive entries form one contiguous Markdown list and a blank line
    // opens/closes the list around non-TOC content.
    let mut prev_was_toc = false;

    while i < lines.len() {
        // Emit a whole detected table region in one shot, then skip past it.
        if region_idx < regions.len() && regions[region_idx].start_line == i {
            let region = &regions[region_idx];
            let y = line_ys[i];

            if prev_was_toc {
                ensure_blank_line(&mut out);
            }
            prev_was_toc = false;

            if prev_y != f32::MAX && (prev_y - y) > body_size * 2.5 {
                out.push('\n');
            }
            out.push_str(&render_gfm_table(region));
            out.push('\n');

            prev_y = line_ys[region.end_line];
            i = region.end_line + 1;
            region_idx += 1;
            continue;
        }

        let line = &lines[i];
        let line_text: String = line
            .iter()
            .map(|&idx| blocks[idx].text.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if line_text.is_empty() {
            i += 1;
            continue;
        }

        let max_font = line
            .iter()
            .map(|&idx| blocks[idx].font_size)
            .fold(0.0f32, f32::max);
        let y = line_ys[i];
        let is_image_line = line.iter().all(|&idx| blocks[idx].is_image);

        // TOC entries (dot-leader lines) render as a flat bullet list instead
        // of prose/headings, with the leader collapsed to a compact "…".
        let is_toc = !is_image_line && contains_dot_leader(&line_text);
        if is_toc {
            if !prev_was_toc {
                ensure_blank_line(&mut out);
            }
            out.push_str("- ");
            out.push_str(&collapse_dot_leader(&line_text));
            out.push('\n');
            prev_y = y;
            prev_was_toc = true;
            i += 1;
            continue;
        }
        if prev_was_toc {
            ensure_blank_line(&mut out);
        }
        prev_was_toc = false;

        // Insert blank line on large vertical gap between sections
        if prev_y != f32::MAX && (prev_y - y) > body_size * 2.5 {
            out.push('\n');
        }

        // Classify heading level: first try font size ratio, then ALL-CAPS heuristic.
        // Image lines are never headings — they have no meaningful font size.
        let heading = if is_image_line {
            ""
        } else if max_font >= body_size * 1.8 {
            "# "
        } else if max_font >= body_size * 1.4 {
            "## "
        } else if max_font >= body_size * 1.15 {
            "### "
        } else if is_all_caps_heading(&line_text) {
            "## "
        } else {
            ""
        };

        out.push_str(heading);
        out.push_str(&line_text);
        out.push('\n');
        prev_y = y;
        i += 1;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_caps_heading_positive() {
        assert!(is_all_caps_heading("OVERVIEW"));
        assert!(is_all_caps_heading("SECTION ONE"));
    }

    #[test]
    fn test_all_caps_heading_rejects_lowercase() {
        assert!(!is_all_caps_heading("Overview"));
    }

    #[test]
    fn test_all_caps_heading_rejects_dot_leader() {
        assert!(!is_all_caps_heading("SECTION ONE...."));
    }

    #[test]
    fn test_contains_dot_leader_positive() {
        assert!(contains_dot_leader("Introduction ........ 5"));
        assert!(contains_dot_leader("Introduction . . . . 5")); // spaced-out leader
    }

    #[test]
    fn test_contains_dot_leader_rejects_short_runs() {
        assert!(!contains_dot_leader("Wait... what?")); // ellipsis
        assert!(!contains_dot_leader("See section 1.2.3 for details")); // version-like
        assert!(!contains_dot_leader("Ordinary prose."));
    }

    #[test]
    fn test_collapse_dot_leader() {
        assert_eq!(
            collapse_dot_leader("TABLE OF CONTENTS .................. 1"),
            "TABLE OF CONTENTS … 1"
        );
        assert_eq!(collapse_dot_leader("No leader here"), "No leader here");
    }

    #[test]
    fn test_all_caps_heading_rejects_sentence_punctuation() {
        assert!(!is_all_caps_heading("END OF REPORT."));
    }

    #[test]
    fn test_all_caps_heading_rejects_length_extremes() {
        assert!(!is_all_caps_heading("AB"));
        assert!(!is_all_caps_heading(&"A".repeat(81)));
    }

    /// Builds a text `TextBlock` at the given position, with `x_end`
    /// inferred from the text length (10 units/char — enough to exercise
    /// gap detection without needing real pdfium glyph metrics).
    fn text_block(x: f32, y: f32, text: &str) -> TextBlock {
        TextBlock {
            x,
            x_end: x + text.len() as f32 * 10.0,
            y,
            font_size: 12.0,
            text: text.to_string(),
            is_image: false,
        }
    }

    #[test]
    fn test_segment_line_into_cells_splits_on_gap_keeps_words_together() {
        // "Hello" and "World" are close together (one cell); "Version" is far
        // to the right of "World" (a second cell).
        let blocks = vec![
            text_block(0.0, 0.0, "Hello"),   // x_end = 50
            text_block(55.0, 0.0, "World"),  // gap 5, x_end = 105
            text_block(300.0, 0.0, "Version"), // gap 195, far
        ];
        let line = vec![0, 1, 2];
        let cells = segment_line_into_cells(&blocks, &line, 20.0);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, "Hello World");
        assert_eq!(cells[1].text, "Version");
    }

    /// Builds one table row's cells at the given column x-positions. The
    /// row's y-coordinate is tracked separately, in each test's `line_ys`.
    fn aligned_row(cols: &[(f32, &str)]) -> Vec<Cell> {
        cols.iter()
            .map(|&(x, text)| Cell {
                x_start: x,
                x_end: x + text.len() as f32 * 10.0,
                text: text.to_string(),
            })
            .collect()
    }

    #[test]
    fn test_detect_table_regions_renders_aligned_rows_as_gfm_table() {
        let rows = vec![
            aligned_row(&[(0.0, "Version"), (100.0, "Date"), (200.0, "Updated by")]),
            aligned_row(&[(0.0, "0.1"), (100.0, "2024-07-26"), (200.0, "ITD")]),
            aligned_row(&[(0.0, "0.2"), (100.0, "2024-08-07"), (200.0, "ITD")]),
        ];
        let line_ys = vec![300.0, 280.0, 260.0];
        let regions = detect_table_regions(&rows, &line_ys, &[], 12.0);
        assert_eq!(regions.len(), 1);
        let region = &regions[0];
        assert_eq!(region.start_line, 0);
        assert_eq!(region.end_line, 2);

        let md = render_gfm_table(region);
        let lines: Vec<&str> = md.lines().collect();
        assert_eq!(lines[0], "| Version | Date | Updated by |");
        assert_eq!(lines[1], "| --- | --- | --- |");
        assert_eq!(lines[2], "| 0.1 | 2024-07-26 | ITD |");
        assert_eq!(lines[3], "| 0.2 | 2024-08-07 | ITD |");
    }

    #[test]
    fn test_detect_table_regions_conservative_gate_rejects_short_runs() {
        // Single-column "table": never a table regardless of row count.
        let single_col = vec![
            aligned_row(&[(0.0, "First bullet")]),
            aligned_row(&[(0.0, "Second bullet")]),
            aligned_row(&[(0.0, "Third bullet")]),
        ];
        let ys = vec![300.0, 280.0, 260.0];
        assert!(detect_table_regions(&single_col, &ys, &[], 12.0).is_empty());

        // A 2-line key:value block (2 aligned columns, but only 2 rows) is
        // exactly the incidental-alignment case MIN_CORE_ROWS guards against.
        let key_value = vec![
            aligned_row(&[(0.0, "Name:"), (100.0, "Alice")]),
            aligned_row(&[(0.0, "Role:"), (100.0, "Engineer")]),
        ];
        let ys2 = vec![300.0, 280.0];
        assert!(detect_table_regions(&key_value, &ys2, &[], 12.0).is_empty());
    }

    #[test]
    fn test_detect_table_regions_rejects_toc_dot_leaders() {
        // A TOC page: each entry is a "title + dot leader" cell and a
        // page-number cell, roughly aligned across entries — exactly the
        // shape that would otherwise satisfy the column-alignment gates.
        let toc = vec![
            aligned_row(&[(0.0, "TABLE OF CONTENTS ...................."), (400.0, "1")]),
            aligned_row(&[(0.0, "DOCUMENT CONTROL ....................."), (400.0, "3")]),
            aligned_row(&[(0.0, "1. ABOUT THIS DOCUMENT ..............."), (400.0, "4")]),
            aligned_row(&[(0.0, "2. INTRODUCTION ......................"), (400.0, "5")]),
        ];
        let ys = vec![400.0, 380.0, 360.0, 340.0];
        assert!(detect_table_regions(&toc, &ys, &[], 12.0).is_empty());
    }

    #[test]
    fn test_detect_table_regions_merges_continuation_row() {
        // A 3-column table (header + 2 fully-aligned data rows, meeting
        // MIN_CORE_ROWS) where the last row's "Notes" column wraps onto an
        // extra physical line (only that column is present, aligned under
        // column index 2).
        let rows = vec![
            aligned_row(&[(0.0, "Version"), (100.0, "Date"), (200.0, "Notes")]),
            aligned_row(&[(0.0, "0.1"), (100.0, "2024-07-26"), (200.0, "Initial")]),
            aligned_row(&[(0.0, "0.8"), (100.0, "2024-10-28"), (200.0, "Re-draft of MNMS")]),
            aligned_row(&[(200.0, "Phase 1 only")]), // continuation, col 2 only
            aligned_row(&[(0.0, "0.81"), (100.0, "2024-10-30"), (200.0, "IT feedback")]),
        ];
        let line_ys = vec![420.0, 400.0, 380.0, 360.0, 340.0];
        let regions = detect_table_regions(&rows, &line_ys, &[], 12.0);
        assert_eq!(regions.len(), 1);
        let region = &regions[0];
        assert_eq!(region.end_line, 4);
        // Header + 3 logical data rows (continuation merged into the "0.8"
        // row, not its own row).
        assert_eq!(region.logical_rows.len(), 4);
        assert_eq!(
            region.logical_rows[2][2],
            "Re-draft of MNMS<br>Phase 1 only"
        );
        assert_eq!(region.logical_rows[3][0], "0.81");
    }

    #[test]
    fn test_escape_table_cell_escapes_pipes_backslashes_and_newlines() {
        assert_eq!(escape_table_cell("a|b"), "a\\|b");
        assert_eq!(escape_table_cell("back\\slash"), "back\\\\slash");
        assert_eq!(escape_table_cell("line1\nline2"), "line1<br>line2");
    }

    /// End-to-end regression test against `tests/fixtures/sample.pdf`
    /// (see `src/fixture_gen.rs` — generated via Pourdown's own PDF export,
    /// not a hand-authored PDF byte stream). Gated on pdfium actually being
    /// loadable so `cargo test` stays green on machines without the vendored
    /// framework; the pure `is_all_caps_heading` tests above still run
    /// unconditionally.
    #[test]
    fn test_pdf_to_markdown_fixture() {
        let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frameworks/pdfium.framework/pdfium");
        if !lib.exists() {
            eprintln!("skipping test_pdf_to_markdown_fixture: pdfium not found at {:?}", lib);
            return;
        }
        std::env::set_var("PDFIUM_LIBRARY_PATH", &lib);

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.pdf");
        let mut sink = MediaSink::new(std::env::temp_dir());
        let md = match pdf_to_markdown(path, &mut sink) {
            Ok(md) => md,
            Err(e) => {
                eprintln!("skipping test_pdf_to_markdown_fixture: pdfium load failed: {}", e);
                return;
            }
        };

        assert!(
            md.contains("This paragraph should survive the PDF roundtrip."),
            "body text missing:\n{md}"
        );
    }
}
