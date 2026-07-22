use markdown2pdf::config::ConfigSource;
use pdfium_render::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::media::MediaSink;
use super::ConversionError;

// Guards the one-time initialization of the global pdfium bindings.
static PDFIUM_INIT: Mutex<bool> = Mutex::new(false);

/// Absolute path to the bundled pdfium library, resolved once at startup by
/// `main`'s `.setup()` via Tauri's resource resolver (see `set_pdfium_lib_path`
/// below). Preferred over the exe-relative guesses in `pdfium_lib_path`
/// because it's tied to where the bundler actually placed the DLL rather than
/// guessing beside the exe — the guesses stayed fragile on Windows because
/// the loader and `tauri.conf.json`'s `bundle.resources` layout weren't
/// actually tied together. A plain `OnceLock` (not `env::set_var`, which is
/// `unsafe` as of the Rust 2024 edition and racy against concurrent reads) is
/// the right fit: written once from `setup()`, read lock-free on the
/// `spawn_blocking` conversion thread.
static PDFIUM_RESOLVED_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Records the resource-resolved pdfium path. Called once from `main`'s
/// `.setup()`; a second call is a no-op (first value wins). Only called on
/// Windows today (see main.rs), hence the `allow` on other targets.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn set_pdfium_lib_path(path: PathBuf) {
    let _ = PDFIUM_RESOLVED_PATH.set(path);
}

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
            let bindings = Pdfium::bind_to_library(&lib)
                .map_err(|e| ConversionError(pdfium_load_diagnostics(&lib, &e)))?;
            Pdfium::new(bindings);
            *initialized = true;
        }
    }

    // After initialization, Pdfium::default() reuses the global bindings without re-loading.
    let pdfium = Pdfium::default();

    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| ConversionError(format!("Failed to open PDF: {}", e)))?;

    // Pass 1: extract every page's blocks up front, with no rendering yet, so
    // running headers/footers can be detected by looking across all pages
    // before any single page is rendered.
    let mut pages: Vec<PageContent> = Vec::new();
    for page in doc.pages().iter() {
        pages.push(PageContent {
            blocks: extract_page_blocks(&page, media)?,
            h_rules: collect_horizontal_rules(&page),
            v_rules: collect_vertical_rules(&page),
            height: page.height().value,
        });
    }

    let hf_keys = detect_running_headers_footers(&pages);
    let repeated_images = detect_repeated_images(&pages);
    // A document-wide, length-weighted body font size, used only for heading
    // classification (see `document_body_size`) — more robust than any single
    // page's median, which a reference-/equation-/caption-heavy page can drag
    // below the true body size and cause every line on it to be misread as a
    // heading.
    let heading_body_size = document_body_size(&pages);

    // Pass 2: render each page, dropping any blocks identified as a repeated
    // running header/footer (text or image).
    let mut md = String::new();
    for page in &pages {
        let kept = filter_header_footer_blocks(&page.blocks, page.height, &hf_keys);
        let kept = filter_repeated_images(&kept, &repeated_images);
        md.push_str(&render_page_blocks(&kept, &page.h_rules, &page.v_rules, heading_body_size));
        md.push('\n');
    }

    Ok(md)
}

/// Every path pdfium resolution would consider, in priority order. Shared by
/// `pdfium_lib_path` (which picks the first that exists) and
/// `pdfium_load_diagnostics` (which reports the full list plus which one
/// existed on disk, since a Windows load failure needs that context to
/// distinguish "wrong path" from "right path, missing dependency").
fn pdfium_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Developer or CI override
    if let Ok(p) = std::env::var("PDFIUM_LIBRARY_PATH") {
        candidates.push(PathBuf::from(p));
    }

    // Resolved from Tauri's resource dir at startup (see set_pdfium_lib_path)
    if let Some(p) = PDFIUM_RESOLVED_PATH.get() {
        candidates.push(p.clone());
    }

    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();

        #[cfg(target_os = "macos")]
        {
            // macOS app bundle: MacOS/ -> ../Frameworks/pdfium.framework/pdfium
            candidates.push(dir.join("../Frameworks/pdfium.framework/pdfium"));
            // Dev build: look for libpdfium.dylib placed beside the binary
            candidates.push(dir.join("libpdfium.dylib"));
        }

        #[cfg(target_os = "windows")]
        {
            // Bundled names are arch-specific (see tauri.conf.json
            // bundle.resources), but also accept the legacy shared name and a
            // "resources/" subdir, so this doesn't depend on the exact layout
            // the NSIS/MSI bundler produces.
            for name in ["pdfium-x64.dll", "pdfium-arm64.dll", "pdfium.dll"] {
                candidates.push(dir.join(name));
                candidates.push(dir.join("resources").join(name));
            }
        }

        #[cfg(target_os = "linux")]
        {
            candidates.push(dir.join("libpdfium.so"));
        }
    }

    // Fall back to platform library name in the current working directory
    candidates.push(PathBuf::from(Pdfium::pdfium_platform_library_name()));

    candidates
}

/// Returns the path to the pdfium dynamic library at runtime: the first
/// candidate from `pdfium_candidate_paths` that exists on disk, or the last
/// (bare platform-name) fallback if none do.
fn pdfium_lib_path() -> PathBuf {
    let candidates = pdfium_candidate_paths();
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates.last().cloned().unwrap())
}

/// Builds a rich, user-readable failure report for a pdfium load error. Only
/// runs on the error path (bind_to_library already failed), so it's free to
/// stat every candidate — zero cost on the success path.
fn pdfium_load_diagnostics(attempted: &Path, err: &PdfiumError) -> String {
    let attempted_exists = attempted.exists();
    let raw = err.to_string();

    let mut report = String::new();
    report.push_str("Failed to load the pdfium library.\n");
    report.push_str(&format!(
        "Attempted: {:?} (exists: {})\n",
        attempted, attempted_exists
    ));
    report.push_str(&format!(
        "current_exe: {}\n",
        std::env::current_exe()
            .map(|p| format!("{:?}", p))
            .unwrap_or_else(|_| "<unavailable>".to_string())
    ));
    report.push_str("Candidates tried:\n");
    for c in pdfium_candidate_paths() {
        report.push_str(&format!("  - {:?} (exists: {})\n", c, c.exists()));
    }
    report.push_str(&format!("Raw error: {}\n", raw));
    report.push_str(diagnose_load_error_hint(&raw, attempted_exists));

    report
}

/// Picks the right one-line hint for a pdfium load failure. Split out from
/// `pdfium_load_diagnostics` as a pure string->string helper so each branch
/// is unit-testable without needing a real `PdfiumError` (which `pdfium`
/// only constructs internally). Distinguishes:
/// - a missing file (bundling/resource issue) — from our own `.exists()`
///   check, not the raw error, since a missing-dependency load failure on
///   Windows surfaces as the *same* generic loader text as a missing file;
/// - wrong architecture (Windows error 193, "not a valid Win32 application");
/// - a missing **symbol** (Windows `GetProcAddress`/error 127, or macOS
///   `dlsym`'s "symbol not found") — the library loaded fine but doesn't
///   export something this build's `pdfium-render` version requires, i.e.
///   the bundled PDFium predates the crate's target version. This used to
///   be misreported as a missing MSVC runtime (see below) because both
///   failure modes go through the same generic Windows loader error path;
///   only the symbol-lookup step's own error text tells them apart. See
///   CLAUDE.md's PDFium gotcha entry and `scripts/fetch-pdfium.mjs` for the
///   incident this distinction was added for;
/// - anything else — a genuinely missing runtime dependency (Windows error
///   126), where the VCRUNTIME140 hint still applies.
fn diagnose_load_error_hint(raw: &str, attempted_exists: bool) -> &'static str {
    let raw_lower = raw.to_lowercase();
    if !attempted_exists {
        "Hint: the pdfium library was not found at the resolved path — this looks like a bundling/resource issue."
    } else if raw.contains("193") || raw_lower.contains("not a valid win32 application") {
        "Hint: the pdfium library exists but is the wrong architecture (e.g. an x64 DLL on ARM64, or vice versa)."
    } else if raw.contains("GetProcAddress")
        || raw.contains("DlSym")
        || raw_lower.contains("symbol not found")
    {
        "Hint: the pdfium library exists and loaded, but is missing a symbol this build requires — the bundled PDFium is an older build than the pdfium-render crate this app was compiled against expects. This is a packaging bug (a version mismatch between the bundled PDFium binary and the pdfium-render crate), not something fixable by reinstalling a runtime — please report it."
    } else {
        "Hint: the pdfium library exists but failed to load; a dependency is likely missing — install the Microsoft Visual C++ Redistributable (VCRUNTIME140.dll)."
    }
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

/// Minimum number of ASCII letters an all-caps candidate must contain.
/// Guards against short equation/table fragments like "QK T" or "G D" that
/// otherwise pass every other gate (no lowercase, no sentence punctuation,
/// alphabetic content present) purely because they happen to be short and
/// upper-case.
const ALL_CAPS_MIN_LETTERS: usize = 4;

/// Minimum fraction of a candidate's non-space characters that must be ASCII
/// letters. Guards against lines that are mostly numbers/symbols with a
/// scattered upper-case letter or two, e.g. a table row like
/// "GNMT + RL [38] 24.6 39.92" (letter ratio ~0.3) — a real heading is
/// overwhelmingly letters.
const ALL_CAPS_MIN_LETTER_RATIO: f32 = 0.6;

/// Returns true for short, genuinely-Latin-all-caps lines with no sentence
/// ending — a fallback heading detector for pages where every run shares the
/// same font size (so the font-ratio classifier can't distinguish a heading).
///
/// Requires at least one ASCII uppercase letter, which is what actually makes
/// this "all-CAPS": text with no case distinction at all (CJK, for instance)
/// vacuously satisfies "zero lowercase letters" and would otherwise match
/// every short, unpunctuated line in the document — flooding a CJK-heavy
/// document (e.g. a résumé) with false headings, since CJK simply has no
/// letter case to be "capped". The added letter-count and letter-ratio gates
/// further keep short equation/table fragments (`QK T`, `O (1) O (1)`,
/// `(A) 4 128 128`, a `GNMT + RL [38] 24.6 39.92` table row) from qualifying —
/// they're short on letters or heavy on symbols/digits, unlike a real
/// ALL-CAPS section title (`ABSTRACT`, `INDEX TERMS`).
fn is_all_caps_heading(text: &str) -> bool {
    let char_count = text.chars().count();
    // Length guard: too short or too long to be a section heading
    if !(3..=80).contains(&char_count) {
        return false;
    }
    // Dot-leader lines are TOC entries, not headings
    if contains_dot_leader(text) {
        return false;
    }
    // A line that's a two-column extraction artifact (left column text
    // duplicated verbatim as the right column) is never a real heading, no
    // matter how all-caps it looks — see `is_duplicated_halves`.
    if is_duplicated_halves(text) {
        return false;
    }
    // An author byline is all-caps like a real heading but carries a mid-line
    // single-letter initial ("W.", "J. G.") — see `looks_like_author_byline`.
    if looks_like_author_byline(text) {
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
    if alpha_count == 0 {
        return false;
    }
    // Must be genuinely Latin-cased, not just case-insensitive (CJK etc).
    let has_upper = text.chars().any(|c| c.is_ascii_uppercase());
    if !has_upper {
        return false;
    }
    let ascii_letter_count = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    if ascii_letter_count < ALL_CAPS_MIN_LETTERS {
        return false;
    }
    let non_space_count = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    (ascii_letter_count as f32 / non_space_count as f32) >= ALL_CAPS_MIN_LETTER_RATIO
}

/// True if `text`'s whitespace-separated tokens split into two equal-length
/// halves that are identical, e.g. `"AIS AIS"` or
/// `"EN-DE EN-FR EN-DE EN-FR"` (tokens `[EN-DE, EN-FR]` == `[EN-DE, EN-FR]`).
///
/// This is the signature of a two-column PDF extraction artifact: pdfium
/// sometimes merges a line's left-column and right-column runs into one
/// logical line where the right column happens to repeat the left column's
/// text verbatim (seen in a table whose columns pdfium read as duplicated
/// text rather than genuine cells). Deliberately an *exact* halves match
/// (not "any repeated token") so an ordinary title that happens to reuse a
/// word ("… AND … AND …") is untouched — only a line that is *literally* two
/// copies of the same run back-to-back qualifies.
fn is_duplicated_halves(text: &str) -> bool {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < 2 || !tokens.len().is_multiple_of(2) {
        return false;
    }
    let (first, second) = tokens.split_at(tokens.len() / 2);
    first == second
}

/// True if `text` contains a mid-line single-letter initial — a bare ASCII
/// uppercase letter followed by `.`, at token index ≥ 1 — the shape of an
/// author byline like `"RICHARD W. ZIOLKOWSKI"` (`W.` at index 1) or
/// `"AND NELSON J. G. FONSECA"` (`J.` at index 2).
///
/// The index-≥1 restriction is what keeps this from rejecting a real lettered
/// subsection heading, whose own letter-dot label sits at index 0
/// (`"A. RUZE LENS"`, `"I. INTRODUCTION"`) — those are left alone.
fn looks_like_author_byline(text: &str) -> bool {
    text.split_whitespace()
        .skip(1)
        .any(|tok| tok.len() == 2 && tok.ends_with('.') && tok.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

#[derive(Clone)]
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
    /// The run's `/BaseFont` name, e.g. "ZYPRQG+CMMI10" (subset-prefixed) or
    /// "Times-Roman" — empty for image blocks. Used only by [`is_math_font`]
    /// to detect math-typeset runs (Computer Modern / AMS or MathType math
    /// families) for best-effort inline/display equation demarcation; never
    /// used for heading/table geometry.
    font_name: String,
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

/// Derives a content-addressed media key for an extracted image: a 64-bit
/// SipHash of `bytes` (via `std::hash::DefaultHasher`, no new dependency)
/// prefixed with the byte length as a cheap extra collision guard. Two pages
/// embedding byte-identical images (e.g. a repeated header/footer logo) get
/// the *same* key, so `MediaSink::add`'s existing de-dup-by-key collapses
/// them to a single written file and a single `![]()` link text — which in
/// turn is what lets [`detect_repeated_images`] recognize the same image
/// recurring across pages (it compares rendered link text, not raw bytes).
fn content_image_key(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("pdf-img-{}-{:016x}.png", bytes.len(), hasher.finish())
}

/// Caption labels a figure/table caption line commonly starts with, checked
/// longest-first so "Figure 3" isn't mistaken for the shorter "Fig" prefix
/// (see [`is_caption_label`]).
const CAPTION_LABEL_PREFIXES: &[&str] = &["Figure", "Fig.", "Fig", "Table", "表", "圖"];

/// True if `text` starts with a figure/table caption label ([`CAPTION_LABEL_PREFIXES`])
/// immediately followed by a numeral (e.g. "Figure 3: …", "Table 1.",
/// "圖 2"), used by [`with_caption_alt`] to associate a caption line with an
/// adjacent image block's alt text. Requires a digit right after the label
/// (allowing one space) so an unrelated sentence starting with "Table" or
/// "Figure" as an ordinary word ("Table tennis is popular") doesn't match.
/// Case-insensitive: confirmed against a real IEEE-style paper, which
/// typesets caption labels in ALL CAPS ("FIGURE 1.", "TABLE 1.") rather than
/// the Title Case used elsewhere ("Figure 1:").
fn is_caption_label(text: &str) -> bool {
    let upper = text.trim_start().to_ascii_uppercase();
    for &prefix in CAPTION_LABEL_PREFIXES {
        if let Some(rest) = upper.strip_prefix(&prefix.to_ascii_uppercase()) {
            if rest.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Escapes a caption for use as a Markdown image's alt text: brackets would
/// otherwise prematurely close the `![…]` span, so they're swapped for
/// parens rather than backslash-escaped (simpler, and captions containing a
/// literal bracket are rare enough that the fidelity loss is negligible).
fn escape_alt_text(text: &str) -> String {
    text.replace('[', "(").replace(']', ")")
}

/// Enriches an already-rendered bare `![]()` image link (`image_md`, the
/// text of the image line at `line_texts[i]`) with an adjacent figure/table
/// caption's text as its alt attribute — e.g. `![Figure 3: Model
/// architecture](assets/fig.png)` — so an image gets meaningful alt text
/// from layout alone, no LLM captioning needed. Prefers the following line
/// (the common "image, then caption below it" layout) and falls back to the
/// preceding one. The
/// caption line itself is left in place and still rendered on its own line
/// immediately after/before — this only adds alt text, it doesn't merge or
/// remove anything, so reading order is unaffected. A no-op (returns
/// `image_md` unchanged) if `image_md` isn't a bare `![]()` link (e.g. the
/// `*(unsupported image)*` placeholder for an unrenderable vector image) or
/// no adjacent line matches [`is_caption_label`].
fn with_caption_alt(image_md: &str, line_texts: &[String], i: usize) -> String {
    if !image_md.starts_with("![](") {
        return image_md.to_string();
    }
    let next = line_texts.get(i + 1).map(|s| s.trim());
    let prev = if i > 0 { line_texts.get(i - 1).map(|s| s.trim()) } else { None };
    let caption = next
        .filter(|s| is_caption_label(s))
        .or_else(|| prev.filter(|s| is_caption_label(s)));
    match caption {
        Some(cap) => image_md.replacen("![](", &format!("![{}](", escape_alt_text(cap)), 1),
        None => image_md.to_string(),
    }
}

/// True if `text` has a TOC-style leader: either a literal-dot run (see
/// [`contains_dot_leader`]) or a real ellipsis glyph (U+2026). PDFs often
/// extract a rendered "…" character rather than a run of "." glyphs, and that
/// case is *not* a dot run, so `contains_dot_leader` alone misses it — this
/// is the union used specifically for TOC *region* detection ([`detect_toc_regions`]),
/// so `contains_dot_leader` itself stays literal-dots-only everywhere else
/// (no risk of a stray "…" in ordinary prose being treated as a leader).
fn has_leader(text: &str) -> bool {
    contains_dot_leader(text) || text.contains('…')
}

/// True if `text` starts with a section-number prefix like "13.", "13.1." or
/// "13.1.2", followed by whitespace (or end of line) — the shape of a TOC
/// entry's leading number. The prefix run is digits and dots only, so a date
/// like "2024-07-26" (a dash breaks the run before any following space) or a
/// letter-led heading like "13.G" (no space after the run) don't match.
fn starts_with_section_number(text: &str) -> bool {
    let t = text.trim_start();
    let prefix_len = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .count();
    if prefix_len == 0 || !t[..prefix_len].chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    t[prefix_len..].is_empty() || t[prefix_len..].starts_with(char::is_whitespace)
}

/// Math/Greek symbols that disqualify a numbered-prefix line from being a
/// section-heading title (see [`numbered_heading_level`]) — their presence
/// means the line is an inlined equation or formula fragment (e.g. GAN's
/// "4.1 Global Optimality of p g = p data", Attention's stray
/// "1 X m h ∇ θ d log D x m i =1 end for"), not prose. Deliberately a curated
/// symbol set rather than "any non-letter" — a title can still contain
/// ordinary punctuation like colons, hyphens, or parentheses.
const HEADING_TITLE_REJECT_SYMBOLS: &[char] = &[
    '=', '∇', '∼', '∈', '×', '√', '·', '∗', '∞', '≤', '≥', '−', '±', '∑', '∏', '∂', '∫',
];

/// Above this fraction of whitespace-separated tokens being a single
/// character, a numbered-prefix line's title is treated as an equation
/// fragment rather than a heading (see [`numbered_heading_level`]) — ordinary
/// section titles are mostly multi-character words, while inlined math
/// (`"1 X m h ∇ θ d log D x m i =1 end for"`) is mostly lone symbols/letters.
const HEADING_TITLE_MAX_SINGLE_CHAR_TOKEN_FRACTION: f32 = 0.4;

/// True if any character in `c` is in the Greek and Coptic Unicode block
/// (U+0370–U+03FF) — math variable names like `θ`, `β`, `α` that mark a line
/// as an equation fragment rather than heading prose.
fn contains_greek(text: &str) -> bool {
    text.chars().any(|c| ('\u{0370}'..='\u{03FF}').contains(&c))
}

/// True if `name` — a PDF run's `/BaseFont`, possibly subset-prefixed like
/// "ZYPRQG+CMMI10" — belongs to one of the math-symbol font families used to
/// typeset equations rather than body prose, confirmed against two real
/// failing samples: LaTeX/Computer-Modern-and-AMS output ("CMMI10",
/// "CMSY7", "CMEX10", "MSBM10" — a Generative Adversarial Networks paper) and
/// MathType output ("RMTMI", "MTSYN", "MTEX" — an IEEE-style antenna review).
/// Deliberately prefix-based rather than an exact-name allowlist, since the
/// subset suffix digit (e.g. "CMMI10" vs "CMMI7") varies by point size and
/// weight. Note "CMBX" (Computer Modern Bold Extended, an ordinary bold text
/// face) is *not* a prefix match here — only the math-symbol families are.
fn is_math_font(name: &str) -> bool {
    let stripped = name.rsplit_once('+').map(|(_, rest)| rest).unwrap_or(name);
    let upper = stripped.to_ascii_uppercase();
    const MATH_FONT_PREFIXES: &[&str] = &[
        "CMMI", "CMSY", "CMEX", "CMBSY", "MSBM", "MSAM", // Computer Modern / AMS math
        "RMTMI", "MTSY", "MTEX", "MTMI", // MathType
        "SYMBOL", "MATHJAX", "STIX",
    ];
    MATH_FONT_PREFIXES.iter().any(|p| upper.starts_with(p)) || upper.contains("MATH")
}

/// Math-only Unicode symbols used as a fallback math signal when a run's
/// font name isn't recognized by [`is_math_font`] (e.g. a math run embedded
/// in a body-text font, or a PDF that doesn't subset a dedicated math font at
/// all). Deliberately the same curated set as [`HEADING_TITLE_REJECT_SYMBOLS`]
/// — both exist to recognize "this text is a formula, not prose" — but kept
/// as a separate constant since the two call sites classify different things
/// (a heading candidate's title vs. an arbitrary run) and may need to diverge.
const MATH_SYMBOL_CHARS: &[char] = &[
    '=', '∇', '∼', '∈', '×', '√', '·', '∗', '∞', '≤', '≥', '−', '±', '∑', '∏', '∂', '∫', '÷', '→',
];

/// True if `c` is a math-only symbol ([`MATH_SYMBOL_CHARS`]) or a Greek
/// letter ([`contains_greek`]) — the fallback signal `block_is_math` uses
/// when font-name matching doesn't apply.
fn is_math_symbol_char(c: char) -> bool {
    MATH_SYMBOL_CHARS.contains(&c) || ('\u{0370}'..='\u{03FF}').contains(&c)
}

/// Fraction of `text`'s non-whitespace characters that are math-only symbols
/// or Greek letters (see [`is_math_symbol_char`]).
fn math_symbol_density(text: &str) -> f32 {
    let total = text.chars().filter(|c| !c.is_whitespace()).count();
    if total == 0 {
        return 0.0;
    }
    let math_chars = text.chars().filter(|&c| is_math_symbol_char(c)).count();
    math_chars as f32 / total as f32
}

/// Above this math-symbol density, a run with no recognized math font name
/// is still treated as math (see [`block_is_math`]) — e.g. a formula
/// rendered in a body-text font, or one whose subset name isn't in
/// [`is_math_font`]'s list. Below it, a stray symbol or two in ordinary
/// prose (an em dash, a multiplication sign in "4×6") isn't enough.
const MATH_SYMBOL_DENSITY_THRESHOLD: f32 = 0.3;

/// True if `block` is part of a math run: its font is a recognized math
/// family ([`is_math_font`]), or — as a fallback for math typeset in a
/// non-dedicated font — its text is dense in math symbols/Greek letters
/// ([`math_symbol_density`] ≥ [`MATH_SYMBOL_DENSITY_THRESHOLD`]). Image
/// blocks are never math.
fn block_is_math(block: &TextBlock) -> bool {
    if block.is_image {
        return false;
    }
    if is_math_font(&block.font_name) {
        return true;
    }
    let text = block.text.trim();
    !text.is_empty() && math_symbol_density(text) >= MATH_SYMBOL_DENSITY_THRESHOLD
}

/// Fraction of a line's non-whitespace character count that comes from
/// math blocks (see [`block_is_math`]), weighted by character count so a
/// long math run outweighs a short adjacent label. Used to decide whether a
/// whole line is a display equation ([`DISPLAY_MATH_MIN_RATIO`]) rather than
/// ordinary prose carrying an inline formula fragment.
fn line_math_ratio(blocks: &[TextBlock], line: &[usize]) -> f32 {
    let mut math_chars = 0usize;
    let mut total_chars = 0usize;
    for &idx in line {
        let block = &blocks[idx];
        if block.is_image {
            continue;
        }
        let n = block.text.trim().chars().filter(|c| !c.is_whitespace()).count();
        total_chars += n;
        if block_is_math(block) {
            math_chars += n;
        }
    }
    if total_chars == 0 {
        0.0
    } else {
        math_chars as f32 / total_chars as f32
    }
}

/// Minimum [`line_math_ratio`] for a whole line to be treated as a display
/// equation (`$$…$$`) rather than prose with an inline formula. High enough
/// that an ordinary sentence mentioning one or two math variables doesn't
/// qualify — a display equation is typically *all* math with at most a
/// trailing equation number.
const DISPLAY_MATH_MIN_RATIO: f32 = 0.6;

/// If `text` ends with a parenthesized run of digits/dots (an equation
/// number like "(3)" or "(3.1)"), splits it off and returns
/// `(body_without_number, Some(number_with_parens))`. Otherwise returns
/// `(text, None)`. Used to keep a display equation's number outside the
/// `$$…$$` delimiters (`$$E = mc^2$$ (3)`) rather than inside them.
fn split_trailing_eq_number(text: &str) -> (&str, Option<&str>) {
    let trimmed = text.trim_end();
    if let Some(rest) = trimmed.strip_suffix(')') {
        if let Some(open) = rest.rfind('(') {
            let inner = &rest[open + 1..];
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return (trimmed[..open].trim_end(), Some(&trimmed[open..]));
            }
        }
    }
    (text, None)
}

/// Rebuilds a line's rendered text from its blocks, mirroring the plain
/// space-join `render_region` uses for `line_texts`, but wraps each
/// contiguous run of math blocks (see [`block_is_math`]) in `$…$` —
/// best-effort inline equation demarcation. Symbols/Unicode are preserved as
/// extracted, not converted to LaTeX (a font-position heuristic can't
/// reconstruct real LaTeX — see the PDF math limitations in CLAUDE.md /
/// markdown-import.md). A line with no math blocks renders identically to
/// the plain join.
fn render_line_with_inline_math(blocks: &[TextBlock], line: &[usize]) -> String {
    let mut groups: Vec<(bool, String)> = Vec::new();
    for &idx in line {
        let block = &blocks[idx];
        let text = block.text.trim();
        if text.is_empty() {
            continue;
        }
        let is_math = block_is_math(block);
        match groups.last_mut() {
            Some((last_math, last_text)) if *last_math == is_math => {
                last_text.push(' ');
                last_text.push_str(text);
            }
            _ => groups.push((is_math, text.to_string())),
        }
    }
    groups
        .into_iter()
        .map(|(is_math, text)| if is_math { format!("${}$", text) } else { text })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns the Markdown heading prefix (`"## "`, `"### "`, `"#### "`, …) for
/// `text` if it is a numbered section heading like "3.2 Attention" or
/// "3.2.1 Scaled Dot-Product Attention", or `None` otherwise.
///
/// This exists because many real headings — especially subsection headings —
/// are typeset in the *same* font size as body text (only bold), so the
/// font-size-ratio classifier in `render_region` can never see them: it has
/// no signal to work with. A numbered prefix followed by a Title-Case,
/// non-mathematical run of words is a strong, font-size-independent signal
/// that the line is a heading, and its depth (number of dot-separated numeric
/// components: "1" -> 1, "3.1" -> 2, "3.2.1" -> 3) gives a natural nesting
/// level — more reliable than incidental font size for this shape of line.
///
/// Guards, in order:
/// - [`starts_with_section_number`] must hold (numeric prefix + trailing
///   whitespace/EOL) — already rejects dates ("2024-07-26") and reference
///   markers ("[1]").
/// - The remainder after the prefix (the "title") must be non-empty — drops
///   bare page/table numbers like "705" or an equation label like "(1)".
/// - The title's first character must be an ASCII uppercase letter — the
///   Title-Case convention real section headings follow, which drops
///   duration fragments ("3 年" -> "年" is not uppercase ASCII) and
///   CJK-led lines (not Latin-cased, so not ASCII-uppercase either).
/// - The title must contain no [`HEADING_TITLE_REJECT_SYMBOLS`] and no Greek
///   letters ([`contains_greek`]) — drops inlined equations.
/// - The title's single-character-token fraction must be
///   ≤ [`HEADING_TITLE_MAX_SINGLE_CHAR_TOKEN_FRACTION`] — a second net for
///   equation fragments that a symbol scan alone might miss.
/// - The title must contain no interior sentence boundary
///   ([`has_interior_sentence_boundary`]) — drops a measurement mistaken for a
///   section number whose "title" is actually a wrapped sentence fragment
///   (e.g. "12.75 GHz. Consequently, it realized a high gain…").
fn numbered_heading_level(text: &str) -> Option<&'static str> {
    if !starts_with_section_number(text) {
        return None;
    }
    let t = text.trim_start();
    let prefix_len = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .count();
    let prefix = &t[..prefix_len];
    let title = t[prefix_len..].trim_start();

    if title.is_empty() {
        return None;
    }
    if !title.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if title.chars().any(|c| HEADING_TITLE_REJECT_SYMBOLS.contains(&c)) || contains_greek(title) {
        return None;
    }
    if has_interior_sentence_boundary(title) {
        return None;
    }

    let tokens: Vec<&str> = title.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let single_char_tokens = tokens.iter().filter(|t| t.chars().count() == 1).count();
    if (single_char_tokens as f32 / tokens.len() as f32) > HEADING_TITLE_MAX_SINGLE_CHAR_TOKEN_FRACTION {
        return None;
    }

    // Depth = number of dot-separated numeric components in the prefix
    // ("1" -> 1, "3.1" -> 2, "3.2.1" -> 3). Trailing/stray dots (already
    // guaranteed non-empty by `starts_with_section_number`) can't produce a
    // zero count since the prefix contains at least one digit.
    let depth = prefix.split('.').filter(|p| !p.is_empty()).count().max(1);
    Some(match depth {
        1 => "## ",
        2 => "### ",
        _ => "#### ",
    })
}

/// True if `text` contains a sentence boundary — an ASCII lowercase letter
/// immediately followed by `.`/`?`/`!` and then whitespace — anywhere but at
/// the very end (a trailing boundary is already handled upstream by the
/// caller's own end-of-text checks). Used by [`numbered_heading_level`] to
/// tell a genuine short noun-phrase title
/// ("Convergence of Algorithm 1") from a measurement mistaken for a section
/// number whose "title" is actually a wrapped sentence fragment
/// ("GHz. Consequently, it realized…", "GPUs. Even our base model").
///
/// Deliberately keyed on *lowercase*-before-`.` so an uppercase-initial
/// abbreviation like "U.S. Policy" (the char before each `.` is uppercase)
/// isn't mistaken for a sentence boundary.
fn has_interior_sentence_boundary(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if matches!(chars[i], '.' | '?' | '!')
            && i > 0
            && chars[i - 1].is_ascii_lowercase()
            && chars.get(i + 1).is_some_and(|c| c.is_whitespace())
        {
            return true;
        }
    }
    false
}

/// True if `text`, once its leader punctuation (dots, ellipsis, spaces) is
/// stripped, is a non-empty run of ASCII digits — a detached TOC page-number
/// fragment like "… 65" or "...... 68" that streamed in as its own line.
fn is_bare_page_ref(text: &str) -> bool {
    let stripped: String = text
        .chars()
        .filter(|&c| c != '.' && c != '…' && !c.is_whitespace())
        .collect();
    !stripped.is_empty() && stripped.chars().all(|c| c.is_ascii_digit())
}

/// True if `text` already ends with a page number (last non-space char is a
/// digit) — i.e. a TOC entry that doesn't need a detached page ref merged in.
fn ends_with_page_number(text: &str) -> bool {
    text.trim_end().chars().next_back().is_some_and(|c| c.is_ascii_digit())
}

/// Returns the trailing run of ASCII digits in `text` (e.g. "… 65" -> "65"),
/// or an empty string if `text` doesn't end in digits.
fn trailing_digits(text: &str) -> String {
    text.chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Ensures `out` ends with a blank line (two trailing newlines), unless it's
/// empty. Used to open/close the TOC bullet list around other content
/// without depending on the unrelated vertical-gap blank-line heuristic.
fn ensure_blank_line(out: &mut String) {
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// Flushes an in-progress paragraph accumulator to `out` as a Markdown
/// paragraph (trailing blank line), leaving `paragraph` empty. A no-op if
/// there's nothing accumulated, so call sites can call it unconditionally at
/// every paragraph-break point (heading, list item, table, big vertical gap,
/// region boundary) without checking emptiness themselves.
fn flush_paragraph(out: &mut String, paragraph: &mut String) {
    if !paragraph.is_empty() {
        out.push_str(paragraph);
        out.push_str("\n\n");
        paragraph.clear();
    }
}

/// True if `text` looks like a bullet or numbered list item, so it stays out
/// of paragraph reflow (each item is emitted on its own line rather than
/// being folded into surrounding prose).
fn is_list_marker(text: &str) -> bool {
    let t = text.trim_start();
    let mut chars = t.chars();
    if matches!(chars.next(), Some('•') | Some('-') | Some('*')) {
        return t.chars().nth(1) == Some(' ');
    }
    // "1. " / "1) " / "IV. " style: a short alphanumeric prefix followed by
    // '.' or ')' and a space.
    let prefix_len = t.chars().take_while(|c| c.is_ascii_alphanumeric()).count();
    if prefix_len == 0 || prefix_len > 3 {
        return false;
    }
    let rest = &t[prefix_len..];
    rest.starts_with(". ") || rest.starts_with(") ")
}

/// Appends a wrapped continuation line `next` onto paragraph accumulator
/// `acc`. Normally joins with a single space; but if `acc` ends with a
/// hyphen and `next` starts with a lowercase letter, the hyphen is treated
/// as a PDF line-wrap split and removed so the word rejoins (e.g. "bet-" +
/// "ter" -> "better"). A hyphen followed by an uppercase letter or
/// non-letter is left alone (more likely a genuine hyphenated compound or
/// proper noun boundary than a wrap split) and joined with a space instead.
fn append_wrapped(acc: &mut String, next: &str) {
    let next = next.trim();
    if next.is_empty() {
        return;
    }
    let ends_with_hyphen = acc.ends_with('-') || acc.ends_with('‐');
    let next_starts_lower = next.chars().next().is_some_and(|c| c.is_lowercase());
    if ends_with_hyphen && next_starts_lower {
        acc.pop();
    } else if !acc.is_empty() {
        acc.push(' ');
    }
    acc.push_str(next);
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
/// third confirms it's a genuine repeating column structure. [`is_bordered_grid`]
/// is the exception: a real ruled box around just 2 aligned rows is
/// independent structural evidence standing in for the third row.
const MIN_CORE_ROWS: usize = 3;

/// True if the row range `[y_top, y_bottom]` — whose cells align to
/// `columns` — sits inside an actual ruled box: a horizontal rule just above
/// the first row, one just below the last, and at least one vertical rule
/// strictly between the outermost columns (an interior column divider).
/// Used by [`detect_table_regions`] to accept a table with only 2
/// column-aligned rows (rather than [`MIN_CORE_ROWS`]'s 3) when the source
/// PDF drew a real box around it — confirmed present in real bordered-table
/// PDFs (e.g. the GAN paper's results table) where a short table legitimately
/// has only 2-3 data rows and the borderless alignment gate alone is slower
/// to trust it. A no-op when the page has no ruling lines at all (`h_rules`/
/// `v_rules` empty, the common borderless case), so this never loosens
/// borderless-table behavior.
fn is_bordered_grid(
    y_top: f32,
    y_bottom: f32,
    columns: &[f32],
    h_rules: &[f32],
    v_rules: &[f32],
    tol: f32,
) -> bool {
    if columns.len() < 2 {
        return false;
    }
    // Rule lines are collected at the midpoint of each drawn segment (see
    // `collect_horizontal_rules`/`collect_vertical_rules`), so "just above/
    // below" allows a couple of tolerance-widths of slack rather than an
    // exact match.
    let margin = tol * 2.0;
    let has_top_rule = h_rules.iter().any(|&y| y > y_top && y <= y_top + margin);
    let has_bottom_rule = h_rules.iter().any(|&y| y < y_bottom && y >= y_bottom - margin);
    let min_x = columns.iter().cloned().fold(f32::MAX, f32::min);
    let max_x = columns.iter().cloned().fold(f32::MIN, f32::max);
    let has_interior_v_rule = v_rules.iter().any(|&x| x > min_x + tol && x < max_x - tol);
    has_top_rule && has_bottom_rule && has_interior_v_rule
}

/// Detects table regions across a page's visual lines using conservative
/// geometry clustering: a region only starts where at least `MIN_CORE_ROWS`
/// consecutive lines share the same ≥2 column positions (the "core" rows) —
/// or, if the range is confirmed enclosed by a ruled box ([`is_bordered_grid`]),
/// just 2 — then extends with further aligned rows or wrapped continuation
/// lines until neither applies.
fn detect_table_regions(
    rows: &[Vec<Cell>],
    line_ys: &[f32],
    h_rules: &[f32],
    v_rules: &[f32],
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
            let bordered = core_end > i
                && is_bordered_grid(line_ys[i], line_ys[core_end], &columns, h_rules, v_rules, tol);
            if !bordered {
                i += 1;
                continue;
            }
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

/// Collects near-vertical ruling-line x-positions from a page's path
/// objects — the same scan as [`collect_horizontal_rules`], mirrored for the
/// opposite axis (long run in y, negligible change in x), used to confirm a
/// bordered/ruled table box in [`is_bordered_grid`].
fn collect_vertical_rules(page: &PdfPage) -> Vec<f32> {
    let mut v_rules: Vec<f32> = Vec::new();

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
                // Near-vertical: long run in y, negligible change in x.
                if dx < 1.0 && dy > 4.0 {
                    v_rules.push((x + px) / 2.0);
                }
            }
            prev = Some((x, y));
        }
    }

    v_rules.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v_rules
}

/// Minimum number of distinct visual lines that must show independent,
/// non-straddling left/right content before a candidate gutter is accepted.
/// Mirrors `MIN_CORE_ROWS` for table detection: a single widely-spaced line
/// (e.g. a title with generous letter-spacing, or two words of a heading)
/// isn't evidence of a genuine two-column layout — only a *repeated*
/// left/right split across several lines is.
const MIN_TWO_COLUMN_LINES: usize = 4;

/// Minimum fraction of the two-sided lines' total text weight that must fall
/// outside any block straddling the candidate gutter. Guards against a
/// gutter choice that technically clears `MIN_TWO_COLUMN_LINES` but still
/// crosses a lot of text.
const MIN_TWO_COLUMN_COVERAGE: f32 = 0.55;

/// Minimum share of non-straddling blocks that must land on each side of
/// the gutter, so a handful of stray blocks on one side don't count as a
/// second column.
const MIN_SIDE_SHARE: f32 = 0.2;

/// A block's weight for gutter-coverage purposes: its font size stands in
/// for the vertical extent it occupies (taller text "covers" more of the
/// page height), floored at 1.0 so a degenerate zero-size block still counts.
fn gutter_weight(b: &TextBlock) -> f32 {
    b.font_size.max(1.0)
}

/// Attempts to find a vertical "gutter" x-coordinate splitting the page
/// into two text columns (the layout IEEE Access and similar journals use).
///
/// Text is first grouped into coarse visual lines by y proximity (a rough
/// pass — the real line grouping happens later in `render_region`; this one
/// only needs to be good enough to count lines). For each candidate split
/// point across the middle of the page's text width, a line counts as
/// "two-sided" only if it has content on both sides with none of its blocks
/// straddling the candidate. Real two-column body text produces many
/// two-sided lines in a row; an ordinary single-column page — even one
/// where per-word text runs leave wide incidental gaps on a title or
/// short line — produces at most a couple, incidentally. Requiring
/// `MIN_TWO_COLUMN_LINES` of them before accepting a candidate is what
/// tells the two apart.
fn detect_gutter(blocks: &[TextBlock]) -> Option<f32> {
    let text_indices: Vec<usize> = (0..blocks.len()).filter(|&i| !blocks[i].is_image).collect();
    if text_indices.len() < 8 {
        return None;
    }

    let mut sizes: Vec<f32> = blocks.iter().map(|b| b.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let body_size = sizes[sizes.len() / 2].max(1.0);
    let line_thresh = body_size * 0.6;

    let mut order = text_indices.clone();
    order.sort_by(|&a, &b| {
        blocks[b]
            .y
            .partial_cmp(&blocks[a].y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for &i in &order {
        if let Some(last) = lines.last_mut() {
            if (blocks[last[0]].y - blocks[i].y).abs() <= line_thresh {
                last.push(i);
                continue;
            }
        }
        lines.push(vec![i]);
    }
    if lines.len() < MIN_TWO_COLUMN_LINES {
        return None;
    }

    // A dot-leader (TOC) line often extracts as many separate short "."
    // text runs rather than one merged block, so no single block spans wide
    // enough to "straddle" a candidate gutter on its own — yet the line
    // still has content near both edges of the page (title on the left,
    // page number on the right), which otherwise looks exactly like
    // evidence for a genuine two-column split. Excluding such lines from
    // the two-sided tally (rather than tightening the general straddle
    // check, which would weaken real gutter detection) keeps TOC pages from
    // being misread as two-column layout — see the IEEE Access regression
    // this function otherwise guards for.
    let line_is_dot_leader: Vec<bool> = lines
        .iter()
        .map(|line| {
            let mut sorted = line.clone();
            sorted.sort_by(|&a, &b| {
                blocks[a].x.partial_cmp(&blocks[b].x).unwrap_or(std::cmp::Ordering::Equal)
            });
            let joined: String = sorted.iter().map(|&i| blocks[i].text.trim()).collect();
            contains_dot_leader(&joined)
        })
        .collect();

    let x_min = text_indices.iter().map(|&i| blocks[i].x).fold(f32::MAX, f32::min);
    let x_max = text_indices
        .iter()
        .map(|&i| blocks[i].x_end)
        .fold(f32::MIN, f32::max);
    let width = x_max - x_min;
    if width <= 0.0 {
        return None;
    }

    let margin = 4.0;
    let steps = 60;
    let mut best: Option<(f32, usize, f32)> = None; // (candidate x, two-sided line count, coverage)
    for step in 0..=steps {
        let frac = 0.35 + 0.30 * (step as f32 / steps as f32);
        let cand = x_min + width * frac;

        let mut two_sided_lines = 0usize;
        let mut left_count = 0usize;
        let mut right_count = 0usize;
        let mut straddle_weight = 0.0f32;
        let mut total_weight = 0.0f32;

        for (line_idx, line) in lines.iter().enumerate() {
            if line_is_dot_leader[line_idx] {
                continue;
            }
            let mut has_left = false;
            let mut has_right = false;
            let mut line_straddles = false;
            // Tallied provisionally per line, then only folded into the
            // page-wide left_count/right_count share below if this line
            // turns out to be genuinely two-sided (see comment there).
            let mut line_left = 0usize;
            let mut line_right = 0usize;
            for &i in line {
                let b = &blocks[i];
                let w = gutter_weight(b);
                total_weight += w;
                if b.x < cand - margin && b.x_end > cand + margin {
                    straddle_weight += w;
                    line_straddles = true;
                    continue;
                }
                if (b.x + b.x_end) / 2.0 < cand {
                    has_left = true;
                    line_left += 1;
                } else {
                    has_right = true;
                    line_right += 1;
                }
            }
            if has_left && has_right && !line_straddles {
                two_sided_lines += 1;
                // MIN_SIDE_SHARE only wants to catch "a handful of stray
                // blocks on one side" being mistaken for a second column
                // (see its doc comment) — so it must draw its left/right
                // tally from confirmed two-sided lines only. Folding in
                // one-sided lines (e.g. a run of lines where a figure
                // occupies the left column's height while prose keeps
                // flowing on the right — common on a figure-dense page like
                // the "METASURFACE-BASED TRANSMITARRAYS" section of a real
                // IEEE-style two-column PDF) dilutes the share with content
                // that was never ambiguous about which side it's on, and can
                // sink a genuinely two-column page's share below the
                // threshold even though its two-sided-line evidence and
                // coverage are both excellent.
                left_count += line_left;
                right_count += line_right;
            }
        }

        if two_sided_lines < MIN_TWO_COLUMN_LINES {
            continue;
        }
        let coverage = 1.0 - straddle_weight / total_weight.max(1.0);
        if coverage < MIN_TWO_COLUMN_COVERAGE {
            continue;
        }
        let total_side = (left_count + right_count).max(1) as f32;
        if (left_count as f32 / total_side) < MIN_SIDE_SHARE
            || (right_count as f32 / total_side) < MIN_SIDE_SHARE
        {
            continue;
        }

        let better = best.is_none_or(|(_, best_lines, best_cov)| {
            two_sided_lines > best_lines || (two_sided_lines == best_lines && coverage > best_cov)
        });
        if better {
            best = Some((cand, two_sided_lines, coverage));
        }
    }

    best.map(|(cand, _, _)| cand)
}

/// True if a block spans across the gutter (its left edge is well left of
/// it and its right edge is well right of it) — i.e. it's a full-width run
/// like a title, running header/footer, or wide figure caption, rather than
/// column-confined body text.
fn is_full_width_block(b: &TextBlock, gutter: f32, margin: f32) -> bool {
    b.x < gutter - margin && b.x_end > gutter + margin
}

/// One reading-order region of a page relative to a detected gutter: either
/// a run of full-width lines (rendered as an ordinary single-column block),
/// or a two-column band (left column rendered fully, then right column).
#[derive(Debug)]
enum Region {
    Full(Vec<usize>),
    TwoCol { left: Vec<usize>, right: Vec<usize> },
}

/// Below this x-gap (relative to `body_size`) between the nearest left- and
/// right-of-gutter runs on a two-sided line, the two runs are treated as one
/// logically continuous full-width line that pdfium happened to split at a
/// run boundary near the gutter (e.g. a bold label immediately followed by
/// its text, like "ABSTRACT " + "Agentic AI…" or "INDEX TERMS " + its
/// keyword list) rather than genuine independent column content. A real
/// two-column gutter is a dedicated empty margin band on both columns, so
/// its gap is reliably much wider than this (measured ~20pt real gutters
/// against ~1-9pt run gaps on a real IEEE Access two-column PDF).
const GUTTER_LINE_GAP_FACTOR: f32 = 1.0;

/// Splits a page's blocks into reading-order [`Region`]s around a detected
/// `gutter`. Blocks are grouped into coarse visual lines (by y proximity);
/// a line is a full-width divider (title, running header/footer, wide
/// figure/table, or a heading label immediately followed by its full-width
/// text) when either some block on it individually spans the gutter, or its
/// left- and right-of-gutter content sit close enough together that they
/// read as one continuous run rather than two independent columns — see
/// `GUTTER_LINE_GAP_FACTOR`. Such a line closes out any open two-column
/// band. All other lines (including ones with content on only one side, or
/// two sides separated by a genuine column-width gap) contribute their
/// blocks to the current band's left or right side by which half of the
/// gutter their center falls on.
fn segment_page(blocks: &[TextBlock], gutter: f32, body_size: f32) -> Vec<Region> {
    let margin = body_size * 0.5;

    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by(|&a, &b| {
        blocks[b]
            .y
            .partial_cmp(&blocks[a].y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let line_thresh = body_size * 0.6;
    let mut line_groups: Vec<Vec<usize>> = Vec::new();
    for &i in &order {
        if let Some(last) = line_groups.last_mut() {
            if (blocks[last[0]].y - blocks[i].y).abs() <= line_thresh {
                last.push(i);
                continue;
            }
        }
        line_groups.push(vec![i]);
    }

    let mut regions: Vec<Region> = Vec::new();
    let mut cur_left: Vec<usize> = Vec::new();
    let mut cur_right: Vec<usize> = Vec::new();

    for line in &line_groups {
        let text_indices: Vec<usize> = line
            .iter()
            .copied()
            .filter(|&i| !blocks[i].is_image)
            .collect();
        let is_full_line = !text_indices.is_empty() && {
            let straddles_any = text_indices
                .iter()
                .any(|&i| is_full_width_block(&blocks[i], gutter, margin));
            if straddles_any {
                true
            } else {
                // No single run straddles, so check whether the line has
                // independent content on both sides close enough together
                // to be one continuous run split at the gutter.
                let mut left_reach = f32::MIN;
                let mut right_reach = f32::MAX;
                for &i in &text_indices {
                    let b = &blocks[i];
                    if b.x_end <= gutter {
                        left_reach = left_reach.max(b.x_end);
                    } else if b.x >= gutter {
                        right_reach = right_reach.min(b.x);
                    }
                }
                left_reach != f32::MIN
                    && right_reach != f32::MAX
                    && (right_reach - left_reach) < body_size * GUTTER_LINE_GAP_FACTOR
            }
        };

        if is_full_line {
            if !cur_left.is_empty() || !cur_right.is_empty() {
                regions.push(Region::TwoCol {
                    left: std::mem::take(&mut cur_left),
                    right: std::mem::take(&mut cur_right),
                });
            }
            match regions.last_mut() {
                Some(Region::Full(v)) => v.extend_from_slice(line),
                _ => regions.push(Region::Full(line.clone())),
            }
            continue;
        }

        for &i in line {
            let center = (blocks[i].x + blocks[i].x_end) / 2.0;
            if center < gutter {
                cur_left.push(i);
            } else {
                cur_right.push(i);
            }
        }
    }
    if !cur_left.is_empty() || !cur_right.is_empty() {
        regions.push(Region::TwoCol {
            left: cur_left,
            right: cur_right,
        });
    }

    regions
}

/// Fraction of page height, at both the top and bottom, treated as the
/// "band" scanned for repeated running headers/footers. Deliberately
/// generous — the band alone doesn't decide removal, it only defines the
/// candidate pool; the real guard against stripping body text is the
/// cross-page repeat requirement in [`detect_running_headers_footers`].
const HF_BAND_FRACTION: f32 = 0.12;

/// Minimum number of distinct pages a band line must recur on (with the same
/// normalized text, see [`normalize_hf`]) before it's treated as a running
/// header/footer. Mirrors the "require repeated structural evidence" gate
/// used elsewhere in this file (`MIN_CORE_ROWS`, `MIN_TWO_COLUMN_LINES`) so a
/// one-off heading or title that happens to sit in the margin band isn't
/// removed.
const HF_MIN_PAGES: usize = 3;

/// Normalizes a candidate header/footer line for cross-page comparison: runs
/// of digits collapse to a single `#` (so incrementing page numbers like
/// "18913"/"18914" compare equal), whitespace collapses to single spaces,
/// and case is folded. Non-digit, non-whitespace text (the running title,
/// author strip, "VOLUME 13, 2025", etc.) is otherwise left intact.
fn normalize_hf(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_was_digit = false;
    for c in text.trim().chars() {
        if c.is_ascii_digit() {
            if !prev_was_digit {
                out.push('#');
            }
            prev_was_digit = true;
        } else {
            out.push(c.to_ascii_lowercase());
            prev_was_digit = false;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Groups the non-image blocks lying in the top or bottom `HF_BAND_FRACTION`
/// of a page of the given `height` into visual lines (by y-proximity, same
/// idiom as `render_region`), and returns each line's normalized text
/// alongside the indices (into `blocks`) of the blocks that make it up. This
/// is the shared unit used by both cross-page detection and per-page
/// filtering, so the two always agree on what counts as a "band line".
fn band_lines(blocks: &[TextBlock], height: f32) -> Vec<(String, Vec<usize>)> {
    if height <= 0.0 {
        return Vec::new();
    }
    let top_thresh = height * (1.0 - HF_BAND_FRACTION);
    let bottom_thresh = height * HF_BAND_FRACTION;

    let mut candidates: Vec<usize> = (0..blocks.len())
        .filter(|&i| {
            let b = &blocks[i];
            !b.is_image && !b.text.trim().is_empty() && (b.y >= top_thresh || b.y <= bottom_thresh)
        })
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    candidates.sort_by(|&a, &b| {
        blocks[b]
            .y
            .partial_cmp(&blocks[a].y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Body font size is unknown at this point (this runs before the
    // page-wide median is computed), so fall back to each line's own leading
    // block's font size for the grouping threshold — good enough to keep a
    // single header/footer run together.
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for &i in &candidates {
        if let Some(last) = lines.last_mut() {
            let thresh = blocks[last[0]].font_size.max(1.0) * 0.6;
            if (blocks[last[0]].y - blocks[i].y).abs() <= thresh {
                last.push(i);
                continue;
            }
        }
        lines.push(vec![i]);
    }

    lines
        .into_iter()
        .map(|mut idxs| {
            idxs.sort_by(|&a, &b| {
                blocks[a]
                    .x
                    .partial_cmp(&blocks[b].x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let text = idxs
                .iter()
                .map(|&i| blocks[i].text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            (normalize_hf(&text), idxs)
        })
        .collect()
}

/// Scans every page's margin bands and returns the normalized text of every
/// line that recurs on at least `HF_MIN_PAGES` distinct pages — the running
/// headers/footers to strip. Each page contributes its band lines as a
/// *set* (deduped) before tallying, so a line repeated multiple times within
/// a single page can't satisfy the cross-page threshold on its own.
fn detect_running_headers_footers(pages: &[PageContent]) -> HashSet<String> {
    let mut page_counts: HashMap<String, usize> = HashMap::new();
    for page in pages {
        let mut seen_this_page: HashSet<String> = HashSet::new();
        for (text, _) in band_lines(&page.blocks, page.height) {
            if !text.is_empty() {
                seen_this_page.insert(text);
            }
        }
        for text in seen_this_page {
            *page_counts.entry(text).or_insert(0) += 1;
        }
    }
    page_counts
        .into_iter()
        .filter(|&(_, count)| count >= HF_MIN_PAGES)
        .map(|(text, _)| text)
        .collect()
}

/// Returns `blocks` with any block belonging to a margin-band line whose
/// normalized text is in `hf_keys` removed. A no-op (returns a full copy)
/// when `hf_keys` is empty, e.g. for a short document below `HF_MIN_PAGES`
/// or a PDF with no repeated running headers/footers at all.
fn filter_header_footer_blocks(
    blocks: &[TextBlock],
    height: f32,
    hf_keys: &HashSet<String>,
) -> Vec<TextBlock> {
    if hf_keys.is_empty() {
        return blocks.to_vec();
    }
    let mut drop: HashSet<usize> = HashSet::new();
    for (text, idxs) in band_lines(blocks, height) {
        if hf_keys.contains(&text) {
            drop.extend(idxs);
        }
    }
    blocks
        .iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(_, b)| b.clone())
        .collect()
}

/// Scans every page's image blocks and returns the `text` (the rendered
/// `![](...)` link, which — thanks to [`content_image_key`] — is identical
/// across pages for byte-identical source images) of every image that
/// recurs on at least `HF_MIN_PAGES` distinct pages: a repeated running
/// header/footer logo or watermark, not incidental content-image reuse.
/// Mirrors [`detect_running_headers_footers`]'s dedupe-per-page-then-tally
/// shape, but is deliberately position-independent (no margin-band
/// restriction) so a centered watermark is caught too, not just a logo
/// confined to the header/footer band.
fn detect_repeated_images(pages: &[PageContent]) -> HashSet<String> {
    let mut page_counts: HashMap<String, usize> = HashMap::new();
    for page in pages {
        let seen_this_page: HashSet<&str> = page
            .blocks
            .iter()
            .filter(|b| b.is_image)
            .map(|b| b.text.as_str())
            .collect();
        for text in seen_this_page {
            *page_counts.entry(text.to_string()).or_insert(0) += 1;
        }
    }
    page_counts
        .into_iter()
        .filter(|&(_, count)| count >= HF_MIN_PAGES)
        .map(|(text, _)| text)
        .collect()
}

/// Returns `blocks` with any image block whose link text is in
/// `repeated_images` removed. A no-op (returns a full copy) when
/// `repeated_images` is empty.
fn filter_repeated_images(blocks: &[TextBlock], repeated_images: &HashSet<String>) -> Vec<TextBlock> {
    if repeated_images.is_empty() {
        return blocks.to_vec();
    }
    blocks
        .iter()
        .filter(|b| !(b.is_image && repeated_images.contains(&b.text)))
        .cloned()
        .collect()
}

/// One page's extracted content, collected up front (Pass 1) so that
/// [`detect_running_headers_footers`] can look across all pages before any
/// page is rendered.
struct PageContent {
    blocks: Vec<TextBlock>,
    h_rules: Vec<f32>,
    /// Vertical ruling-line x-positions (see [`collect_vertical_rules`]),
    /// paired with `h_rules` to confirm a bordered/ruled table box in
    /// [`is_bordered_grid`]. Empty for a page with no drawn table borders —
    /// the common case — which keeps `detect_table_regions` behaving exactly
    /// as before this field existed.
    v_rules: Vec<f32>,
    /// Page height in PDF points, used to define the top/bottom margin bands
    /// scanned for repeated running headers/footers.
    height: f32,
}

/// Strips the Unicode replacement character (U+FFFD), which pdfium emits in
/// place of any glyph lacking a ToUnicode mapping — it never legitimately
/// appears in real document text, only as an "undecodable glyph" marker. Left
/// in place, it shows up verbatim in extracted text and, worse, inside a
/// `$…$`/`$$…$$` math span (see [`block_is_math`]) where it's guaranteed to
/// break KaTeX parsing with an "Unexpected character" error (seen on the GAN
/// paper's Algorithm 1, whose formulas are typeset in a Computer Modern math
/// font pdfium couldn't fully map to Unicode).
fn strip_undecodable(text: &str) -> String {
    text.chars().filter(|&c| c != '\u{FFFD}').collect()
}

/// Extracts one page's positioned text/image blocks (no layout analysis or
/// rendering yet). Runs once per page in Pass 1, before cross-page
/// header/footer detection.
fn extract_page_blocks(
    page: &PdfPage,
    media: &mut MediaSink,
) -> Result<Vec<TextBlock>, ConversionError> {
    let mut blocks: Vec<TextBlock> = Vec::new();

    for obj in page.objects().iter() {
        if let Some(text_obj) = obj.as_text_object() {
            let text = strip_undecodable(&text_obj.text());
            if text.trim().is_empty() {
                continue;
            }
            let matrix = match text_obj.matrix() {
                Ok(m) => m,
                Err(_) => continue,
            };
            // `scaled_font_size`, not `unscaled_font_size`: some PDFs (seen
            // in a two-column IEEE/OJAP-style journal export) bake the
            // actual point size into the text object's transformation
            // matrix rather than the font's own size parameter, so
            // `unscaled_font_size` reports a uniform 1.0pt for every run
            // regardless of its true rendered size. That collapses
            // `body_size` (the median below) to 1.0 on every page, which
            // cascades into every size-relative threshold in this file —
            // most visibly `segment_page`'s gutter straddle margin
            // (`body_size * 0.5`), which becomes sub-pixel and misreads
            // ordinary column-edge line endings as full-width lines
            // spanning the gutter, collapsing the whole two-column body
            // into one region and letting `detect_table_regions` see the
            // interleaved left/right lines as a 2-column table.
            // `scaled_font_size` multiplies by the matrix's vertical scale,
            // recovering the true rendered size in both cases.
            let font_size = text_obj.scaled_font_size().value;
            let x = matrix.e();
            // Right edge of the run's bounding box, used for table-cell gap
            // detection. Falls back to `x` (a zero-width block) if pdfium
            // can't report bounds for this object.
            let x_end = obj.bounds().map(|b| b.right().value).unwrap_or(x);
            let font_name = text_obj.font().name();
            blocks.push(TextBlock {
                x,
                x_end,
                y: matrix.f(),
                font_size,
                text,
                is_image: false,
                font_name,
            });
        } else if let Some(image_obj) = obj.as_image_object() {
            let (x, y) = match obj.matrix() {
                Ok(m) => (m.e(), m.f()),
                Err(_) => (0.0, 0.0),
            };
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
                        // Content-addressed key (not a per-page name): two
                        // pages embedding byte-identical images (e.g. a
                        // repeated header/footer logo) collapse to the same
                        // written asset via `MediaSink`'s de-dup, and their
                        // blocks get identical `text`, which is what lets
                        // `detect_repeated_images` recognize the recurrence.
                        let key = content_image_key(&png_bytes);
                        match media.add(&key, &png_bytes) {
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
                font_name: String::new(),
            });
        }
    }

    Ok(blocks)
}

/// Renders one page's already-extracted (and header/footer-filtered) blocks
/// as Markdown. Runs once per page in Pass 2, after
/// [`detect_running_headers_footers`] has decided what to filter out.
/// A document-wide, length-weighted estimate of the "true" body font size,
/// used only for heading font-ratio classification in `render_region` (never
/// for geometry — line grouping, table/gutter detection keep using each
/// page's own median, which is what they need).
///
/// A single page's median (see `render_page_blocks`) is weighted by *glyph-run
/// count*, not text length, and includes image blocks (stored with
/// `font_size: 0.0`). A page dense with equations, subscripts, figure
/// captions, and reference markers — common in the back half of a long paper
/// — can pack enough small-font runs to drag that median below the true body
/// size, which then makes ordinary body lines clear the heading ratio
/// threshold. Bucketing every text block's font size to the nearest 0.5pt and
/// picking the bucket with the most total *characters* (not run count) finds
/// the size the bulk of the document's prose actually uses, which a handful
/// of caption-/equation-heavy pages can't skew.
fn document_body_size(pages: &[PageContent]) -> f32 {
    let mut weight_by_bucket: HashMap<i32, f32> = HashMap::new();
    for page in pages {
        for block in &page.blocks {
            if block.is_image || block.font_size <= 0.0 {
                continue;
            }
            let bucket = (block.font_size * 2.0).round() as i32;
            let weight = block.text.chars().count().max(1) as f32;
            *weight_by_bucket.entry(bucket).or_insert(0.0) += weight;
        }
    }

    if let Some((&bucket, _)) = weight_by_bucket
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
    {
        return bucket as f32 / 2.0;
    }

    // No text blocks at all (e.g. an image-only document): fall back to the
    // simple per-page median approach used elsewhere.
    let mut sizes: Vec<f32> = pages
        .iter()
        .flat_map(|p| p.blocks.iter())
        .map(|b| b.font_size)
        .filter(|&s| s > 0.0)
        .collect();
    if sizes.is_empty() {
        return 0.0;
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sizes[sizes.len() / 2]
}

fn render_page_blocks(
    blocks: &[TextBlock],
    h_rules: &[f32],
    v_rules: &[f32],
    heading_body_size: f32,
) -> String {
    if blocks.is_empty() {
        return String::new();
    }

    // Determine body (median) font size for relative heading detection
    let mut sizes: Vec<f32> = blocks.iter().map(|b| b.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let body_size = sizes[sizes.len() / 2];
    if body_size <= 0.0 {
        return blocks
            .iter()
            .map(|b| b.text.trim().to_string())
            .collect::<Vec<_>>()
            .join(" ");
    }

    let all_indices: Vec<usize> = (0..blocks.len()).collect();

    // Font-ratio heading detection uses the document-wide baseline, not this
    // page's own median — see `document_body_size`. Fall back to the
    // per-page median for the rare page/document where the doc-wide estimate
    // couldn't be computed (e.g. an image-only document).
    let heading_body_size = if heading_body_size > 0.0 {
        heading_body_size
    } else {
        body_size
    };

    // Two-column journal layouts (IEEE Access and similar) put a left- and
    // right-column line at the same height; grouping purely by y (as
    // `render_region` does within a single region) would merge them into
    // one cross-column "line", corrupting both reading order and — via the
    // wide gap between them — table detection. So layout is decided first:
    // no gutter found means the existing single-region path runs unchanged;
    // a detected gutter splits the page into full-width dividers and
    // two-column bands, each rendered as its own region.
    match detect_gutter(blocks) {
        None => render_region(blocks, &all_indices, body_size, heading_body_size, h_rules, v_rules),
        Some(gutter) => {
            let mut out = String::new();
            for region in segment_page(blocks, gutter, body_size) {
                match region {
                    Region::Full(indices) => {
                        out.push_str(&render_region(blocks, &indices, body_size, heading_body_size, h_rules, v_rules));
                    }
                    Region::TwoCol { left, right } => {
                        out.push_str(&render_region(blocks, &left, body_size, heading_body_size, h_rules, v_rules));
                        out.push_str(&render_region(blocks, &right, body_size, heading_body_size, h_rules, v_rules));
                    }
                }
            }
            out
        }
    }
}

/// Minimum number of leader lines (dot-run or ellipsis-glyph, see
/// [`has_leader`]) required within a run of "continuable" lines before it's
/// accepted as a TOC region. Mirrors `MIN_CORE_ROWS` / `HF_MIN_PAGES` — the
/// same "require repeated structural evidence" gate used elsewhere in this
/// file, so an isolated one-off leader line (e.g. a single "see fig ..... 5"
/// in body text) doesn't get swept into region treatment; it keeps the
/// existing per-line handling in `render_region` instead.
const TOC_MIN_LEADER_LINES: usize = 3;

/// Scans a region's visual lines (already rendered to text, with an
/// `is_image_line` flag per line) for contiguous runs that look like a table
/// of contents, and returns each qualifying run as an inclusive
/// `(start_line, end_line)` index range (both indices into `line_texts` /
/// `is_image_line`).
///
/// A line is "continuable" — i.e. it could plausibly belong to a TOC entry —
/// if it's blank, image-only, has a leader ([`has_leader`]), or starts a new
/// entry ([`starts_with_section_number`]); this also covers wrapped title
/// tails and detached page-number fragments, which have none of those
/// properties on their own but sit *between* qualifying lines within the same
/// maximal run. A run is only promoted to a region if it contains at least
/// `TOC_MIN_LEADER_LINES` leader lines — the same repeated-evidence gate used
/// for table/gutter/header-footer detection — then leading/trailing
/// blank/image-only lines are trimmed off the region's ends so a stray blank
/// line doesn't get swallowed into TOC rendering.
fn detect_toc_regions(line_texts: &[String], is_image_line: &[bool]) -> Vec<(usize, usize)> {
    let n = line_texts.len();
    let is_continuable = |i: usize| -> bool {
        is_image_line[i]
            || line_texts[i].trim().is_empty()
            || has_leader(&line_texts[i])
            || starts_with_section_number(&line_texts[i])
    };

    let mut regions = Vec::new();
    let mut i = 0;
    while i < n {
        if !is_continuable(i) {
            i += 1;
            continue;
        }
        let start = i;
        let mut leader_count = 0usize;
        while i < n && is_continuable(i) {
            if !is_image_line[i] && has_leader(&line_texts[i]) {
                leader_count += 1;
            }
            i += 1;
        }
        let mut end = i - 1;
        if leader_count >= TOC_MIN_LEADER_LINES {
            let mut s = start;
            while s <= end && (is_image_line[s] || line_texts[s].trim().is_empty()) {
                s += 1;
            }
            while end > s && (is_image_line[end] || line_texts[end].trim().is_empty()) {
                end -= 1;
            }
            if s <= end {
                regions.push((s, end));
            }
        }
    }
    regions
}

/// Reflows a detected TOC region (`line_texts[start..=end]`, with
/// `is_image_line` flags) into a uniform Markdown bullet list, one `- ` per
/// logical entry, and returns it (including the trailing blank line that
/// closes the list). Unlike ordinary body text, no heading/list/table
/// classification runs here — a TOC entry is never promoted to a heading no
/// matter its font size or ALL-CAPS shape.
///
/// Each line is first leader-collapsed ([`collapse_dot_leader`], which passes
/// a real "…" glyph through unchanged). Then, in order:
/// - An image line flushes any accumulated entry as a bullet, emits the image
///   on its own line, and resets — a safety net for the rare non-repeated
///   image landing inside a TOC (repeated ones are already stripped
///   upstream), so nothing is silently dropped.
/// - A detached page-number fragment ([`is_bare_page_ref`]) is FIFO-assigned
///   to the earliest still-"needy" entry (the cursor skips past any entry
///   that already [`ends_with_page_number`]) — handles titles and their page
///   numbers streaming in out of line-order.
/// - A line starting a new section number ([`starts_with_section_number`])
///   opens a new entry.
/// - Anything else is a wrapped continuation of the current entry's title,
///   joined with [`append_wrapped`] (hyphen-aware, same as body reflow).
fn render_toc_region(line_texts: &[String], is_image_line: &[bool], start: usize, end: usize) -> String {
    let mut out = String::new();
    let mut entries: Vec<String> = Vec::new();
    let mut needy_cursor = 0usize;

    let flush_entries = |out: &mut String, entries: &mut Vec<String>, needy_cursor: &mut usize| {
        for entry in entries.iter() {
            if entry.is_empty() {
                continue;
            }
            out.push_str("- ");
            out.push_str(entry);
            out.push('\n');
        }
        entries.clear();
        *needy_cursor = 0;
    };

    for idx in start..=end {
        if is_image_line[idx] {
            let text = line_texts[idx].trim();
            if text.is_empty() {
                continue;
            }
            flush_entries(&mut out, &mut entries, &mut needy_cursor);
            out.push_str(text);
            out.push_str("\n\n");
            continue;
        }
        let collapsed = collapse_dot_leader(&line_texts[idx]);
        let text = collapsed.trim();
        if text.is_empty() {
            continue;
        }

        if is_bare_page_ref(text) {
            let digits = trailing_digits(text);
            while needy_cursor < entries.len() && ends_with_page_number(&entries[needy_cursor]) {
                needy_cursor += 1;
            }
            if needy_cursor < entries.len() {
                let entry = &mut entries[needy_cursor];
                if !entry.ends_with(' ') {
                    entry.push(' ');
                }
                entry.push_str(&digits);
                needy_cursor += 1;
            } else {
                entries.push(text.to_string());
            }
            continue;
        }

        if starts_with_section_number(text) || entries.is_empty() {
            entries.push(text.to_string());
        } else {
            append_wrapped(entries.last_mut().expect("checked non-empty"), text);
        }
    }

    flush_entries(&mut out, &mut entries, &mut needy_cursor);
    out.push('\n');
    out
}

/// Returns a character-weighted representative font size for one visual
/// line's non-image blocks, used by `render_region`'s font-ratio heading
/// classifier instead of the line's raw maximum font size.
///
/// A plain maximum is fooled by a drop-cap: one oversized initial glyph on an
/// otherwise body-sized line inflates the line's max far past the heading
/// ratio thresholds, misclassifying ordinary body text as a top-level
/// heading (e.g. a "T" drop cap making "T systems are posing challenges…"
/// read as `# `). Weighting by each run's character count and taking the
/// size at the 50%-cumulative-character mark means a single-character
/// oversized run can't dominate: it's outvoted by the many body-sized
/// characters around it. A genuinely large-font heading, where most or all
/// characters share the big size, still resolves to that size — unchanged
/// from the old maximum for that common case.
fn char_weighted_median_font_size(blocks: &[TextBlock], line: &[usize]) -> f32 {
    let mut sized: Vec<(f32, usize)> = line
        .iter()
        .filter(|&&idx| !blocks[idx].is_image)
        .map(|&idx| (blocks[idx].font_size, blocks[idx].text.chars().count().max(1)))
        .collect();
    if sized.is_empty() {
        return 0.0;
    }
    sized.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total_chars: usize = sized.iter().map(|&(_, n)| n).sum();
    let half = total_chars / 2;
    let mut cumulative = 0usize;
    for &(size, n) in &sized {
        cumulative += n;
        if cumulative > half {
            return size;
        }
    }
    sized.last().map(|&(size, _)| size).unwrap_or(0.0)
}

/// Renders one reading-order region of a page as Markdown — either the
/// whole page (no multi-column layout detected) or a single column of a
/// two-column band. Groups the region's blocks into visual lines top to
/// bottom, detects table regions within it (see [`detect_table_regions`]),
/// and emits headings/lists/tables/images each on their own line, while
/// consecutive plain body lines are reflowed into single paragraphs —
/// de-hyphenating words that wrapped across the PDF's line break (see
/// [`append_wrapped`]).
fn render_region(
    blocks: &[TextBlock],
    indices: &[usize],
    body_size: f32,
    heading_body_size: f32,
    h_rules: &[f32],
    v_rules: &[f32],
) -> String {
    if indices.is_empty() {
        return String::new();
    }

    // Sort top-to-bottom (PDF y=0 is at page bottom, so higher y is higher
    // on page), then left-to-right within a line.
    let mut order: Vec<usize> = indices.to_vec();
    order.sort_by(|&a, &b| {
        let dy = blocks[b]
            .y
            .partial_cmp(&blocks[a].y)
            .unwrap_or(std::cmp::Ordering::Equal);
        if dy == std::cmp::Ordering::Equal {
            blocks[a]
                .x
                .partial_cmp(&blocks[b].x)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            dy
        }
    });

    // Group into visual lines by y proximity
    let line_thresh = body_size * 0.6;
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for &i in &order {
        if let Some(last) = lines.last_mut() {
            if (blocks[last[0]].y - blocks[i].y).abs() <= line_thresh {
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

    // Precompute each line's joined text and image-only flag once, reused by
    // both TOC region detection and the per-line text below (previously
    // recomputed inside the loop).
    let line_texts: Vec<String> = lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|&idx| blocks[idx].text.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let image_line_flags: Vec<bool> = lines
        .iter()
        .map(|line| line.iter().all(|&idx| blocks[idx].is_image))
        .collect();

    // Detect TOC regions up front (see `detect_toc_regions`) so table
    // detection can skip their lines entirely — this also keeps glyph-leader
    // ("…") TOC entries, which `row_has_dot_leader` alone wouldn't catch,
    // from being misread as a 2-column table.
    let toc_regions = detect_toc_regions(&line_texts, &image_line_flags);
    let mut in_toc = vec![false; lines.len()];
    for &(s, e) in &toc_regions {
        for flag in &mut in_toc[s..=e] {
            *flag = true;
        }
    }

    // Detect table regions before emitting: segment each line into cells,
    // then cluster consecutive lines that share ≥2 aligned columns. See
    // `detect_table_regions` for the conservative gating rules.
    let gap_thresh = body_size * 1.2;
    let rows: Vec<Vec<Cell>> = lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            if in_toc[idx] || line.iter().all(|&i| blocks[i].is_image) {
                Vec::new()
            } else {
                segment_line_into_cells(blocks, line, gap_thresh)
            }
        })
        .collect();
    let line_ys: Vec<f32> = lines.iter().map(|line| blocks[line[0]].y).collect();
    let regions = detect_table_regions(&rows, &line_ys, h_rules, v_rules, body_size);

    let mut out = String::new();
    let mut prev_y = f32::MAX;
    let mut region_idx = 0;
    let mut toc_idx = 0;
    let mut i = 0;
    // Tracks whether the previous emitted line was a TOC entry, so
    // consecutive entries form one contiguous Markdown list and a blank line
    // opens/closes the list around other content.
    let mut prev_was_toc = false;
    // Accumulates consecutive plain body lines into one paragraph; flushed
    // (see `flush_paragraph`) at every paragraph-break point.
    let mut paragraph = String::new();

    while i < lines.len() {
        // Emit a whole detected TOC region in one shot, then skip past it —
        // checked before table regions since TOC lines never seed a table
        // (their rows were blanked above).
        if toc_idx < toc_regions.len() && toc_regions[toc_idx].0 == i {
            flush_paragraph(&mut out, &mut paragraph);
            let (start, end) = toc_regions[toc_idx];
            if !prev_was_toc {
                ensure_blank_line(&mut out);
            }
            out.push_str(&render_toc_region(&line_texts, &image_line_flags, start, end));
            prev_y = line_ys[end];
            prev_was_toc = true;
            i = end + 1;
            toc_idx += 1;
            while region_idx < regions.len() && regions[region_idx].start_line <= end {
                region_idx += 1;
            }
            continue;
        }

        // Emit a whole detected table region in one shot, then skip past it.
        if region_idx < regions.len() && regions[region_idx].start_line == i {
            flush_paragraph(&mut out, &mut paragraph);
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
        let line_text = &line_texts[i];
        if line_text.is_empty() {
            i += 1;
            continue;
        }

        let max_font = char_weighted_median_font_size(blocks, line);
        let y = line_ys[i];
        let is_image_line = image_line_flags[i];

        // TOC entries (dot-leader lines) render as a flat bullet list instead
        // of prose/headings, with the leader collapsed to a compact "…".
        let is_toc = !is_image_line && contains_dot_leader(line_text);
        if is_toc {
            flush_paragraph(&mut out, &mut paragraph);
            if !prev_was_toc {
                ensure_blank_line(&mut out);
            }
            out.push_str("- ");
            out.push_str(&collapse_dot_leader(line_text));
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

        // A large vertical gap ends the current paragraph, even if this
        // line turns out to be more plain body text (the start of a new
        // paragraph) rather than a heading/list/image.
        let big_gap = prev_y != f32::MAX && (prev_y - y) > body_size * 2.5;
        if big_gap {
            flush_paragraph(&mut out, &mut paragraph);
        }

        // Computed up front (rather than only when `heading` turns out empty,
        // as before) so the font-ratio shape guard below can see it. Does
        // *not* suppress the ALL-CAPS heading path below — a lettered
        // subsection heading like "J. ROADMAP FOR FUTURE RESEARCH"
        // incidentally matches `is_list_marker`'s "single letter + '. '"
        // shape too, and that ALL-CAPS branch already has its own
        // length/punctuation guards (`is_all_caps_heading`) that make it safe
        // to keep taking priority over the list read, exactly as before this
        // change.
        let is_list = !is_image_line && is_list_marker(line_text);

        // Font-ratio heading candidates must also look like a heading in
        // shape: not a bulleted/numbered list item, short, and not ending in
        // sentence/wrap punctuation. Without this, a font-size baseline
        // that's even slightly off (see `document_body_size`) can promote an
        // ordinary wrapped sentence — or a large-font bullet — to a heading
        // just because its font happens to clear the ratio. The last two
        // guards catch shapes that can otherwise slip past font size alone: a
        // two-column extraction artifact where the line is literally two
        // copies of the same run back-to-back (`is_duplicated_halves`), and
        // an author byline whose mid-line initial happens to sit at a large
        // font size (`looks_like_author_byline`).
        let is_layout_artifact =
            is_duplicated_halves(line_text) || looks_like_author_byline(line_text);
        // clippy's De Morgan's-law rewrite of this chain (one fully negated
        // `||` clause) reads worse than the positive-guard form kept here.
        #[allow(clippy::nonminimal_bool)]
        let heading_shape_ok = !is_list
            && line_text.chars().count() <= 80
            && !(line_text.ends_with('.') || line_text.ends_with(','))
            && !is_layout_artifact;

        // Classify heading level: first try the numbered-section pattern
        // (font-size-independent — see `numbered_heading_level`, which
        // catches subsection headings typeset at body size), then font size
        // ratio (against the document-wide baseline, not this page's own
        // median — see `document_body_size`), then the ALL-CAPS heuristic.
        // Image lines are never headings — they have no meaningful font size.
        let heading = if is_image_line {
            ""
        } else if let Some(level) =
            heading_shape_ok.then(|| numbered_heading_level(line_text)).flatten()
        {
            level
        } else if heading_shape_ok && max_font >= heading_body_size * 1.8 {
            "# "
        } else if heading_shape_ok && max_font >= heading_body_size * 1.4 {
            "## "
        } else if heading_shape_ok && max_font >= heading_body_size * 1.15 {
            "### "
        } else if is_all_caps_heading(line_text) {
            "## "
        } else {
            ""
        };

        // A whole line dominated by math blocks (see `line_math_ratio`) is
        // rendered as its own display equation rather than folded into
        // paragraph prose — but only once it's cleared every other
        // classification above (heading/list/image/TOC), so a numbered
        // subsection heading that happens to contain a formula fragment
        // still renders as a heading, not a `$$…$$` block.
        let is_display_math = !is_image_line
            && !is_list
            && heading.is_empty()
            && line_math_ratio(blocks, line) >= DISPLAY_MATH_MIN_RATIO;

        if !heading.is_empty() || is_image_line || is_list {
            flush_paragraph(&mut out, &mut paragraph);
            out.push_str(heading);
            if is_image_line {
                out.push_str(&with_caption_alt(line_text, &line_texts, i));
            } else {
                out.push_str(line_text);
            }
            out.push_str("\n\n");
        } else if is_display_math {
            flush_paragraph(&mut out, &mut paragraph);
            let (body, eq_number) = split_trailing_eq_number(line_text);
            out.push_str("$$");
            out.push_str(body.trim());
            out.push_str("$$");
            if let Some(n) = eq_number {
                out.push(' ');
                out.push_str(n);
            }
            out.push_str("\n\n");
        } else if paragraph.is_empty() {
            paragraph.push_str(&render_line_with_inline_math(blocks, line));
        } else {
            append_wrapped(&mut paragraph, &render_line_with_inline_math(blocks, line));
        }

        prev_y = y;
        i += 1;
    }
    flush_paragraph(&mut out, &mut paragraph);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_undecodable_removes_replacement_chars() {
        // Reproduces the GAN-paper "Unexpected character: '�'" KaTeX error:
        // pdfium emits U+FFFD for glyphs it can't map to Unicode, which then
        // lands inside a `$$…$$` math span and breaks parsing.
        assert_eq!(strip_undecodable("1 X m h \u{FFFD}"), "1 X m h ");
    }

    #[test]
    fn test_strip_undecodable_all_replacement_chars_collapses_to_empty() {
        assert_eq!(strip_undecodable("\u{FFFD}\u{FFFD}\u{FFFD}"), "");
    }

    #[test]
    fn test_strip_undecodable_leaves_ordinary_text_untouched() {
        assert_eq!(strip_undecodable("∇θ log D(x)"), "∇θ log D(x)");
    }

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
    fn test_all_caps_heading_rejects_equation_fragments() {
        // Real regressions: equation/table fragments that are short,
        // upper-case and unpunctuated, but not genuine ALL-CAPS headings.
        assert!(!is_all_caps_heading("QK T"));
        assert!(!is_all_caps_heading("O (1) O (1)"));
        assert!(!is_all_caps_heading("(A) 4 128 128"));
        assert!(!is_all_caps_heading("G D"));
        assert!(!is_all_caps_heading("GNMT + RL [38] 24.6 39.92"));
    }

    #[test]
    fn test_all_caps_heading_rejects_cjk() {
        // CJK has no letter case, so "zero lowercase letters" is vacuously
        // true for every CJK line — requiring an ASCII uppercase letter is
        // what actually makes this an ALL-CAPS *Latin* heading detector.
        assert!(!is_all_caps_heading("工 作 經 驗"));
        assert!(!is_all_caps_heading("台 北 市 文 山 區"));
    }

    #[test]
    fn test_all_caps_heading_still_accepts_real_headings() {
        assert!(is_all_caps_heading("ABSTRACT"));
        assert!(is_all_caps_heading("INDEX TERMS"));
        assert!(is_all_caps_heading("INTRODUCTION"));
    }

    #[test]
    fn test_all_caps_heading_rejects_duplicated_columns() {
        // Real regressions: a two-column extraction artifact where pdfium
        // merged the left and right column runs into one line, and the
        // right column happened to repeat the left column verbatim.
        assert!(!is_all_caps_heading("AIS AIS"));
        assert!(!is_all_caps_heading("EN-DE EN-FR EN-DE EN-FR"));
    }

    #[test]
    fn test_all_caps_heading_rejects_author_bylines() {
        // Real regressions: author bylines, all-caps like a real heading but
        // carrying a mid-line single-letter initial.
        assert!(!is_all_caps_heading("RICHARD W. ZIOLKOWSKI"));
        assert!(!is_all_caps_heading("AND NELSON J. G. FONSECA"));
    }

    #[test]
    fn test_all_caps_heading_keeps_lettered_subsection_headings() {
        // A real lettered subsection heading has its letter-dot label at
        // token index 0, not mid-line like a byline's initial — must still
        // be accepted.
        assert!(is_all_caps_heading("A. RUZE LENS"));
        assert!(is_all_caps_heading("I. INTRODUCTION"));
    }

    #[test]
    fn test_is_duplicated_halves() {
        assert!(is_duplicated_halves("AIS AIS"));
        assert!(is_duplicated_halves("EN-DE EN-FR EN-DE EN-FR"));
        assert!(is_duplicated_halves("Intractable, may be Intractable, may be"));
        assert!(!is_duplicated_halves("INDEX TERMS"));
        assert!(!is_duplicated_halves("ABSTRACT"));
        assert!(!is_duplicated_halves("ONE"));
        // Odd token count can't split into equal halves.
        assert!(!is_duplicated_halves("ONE TWO THREE"));
    }

    #[test]
    fn test_looks_like_author_byline() {
        assert!(looks_like_author_byline("RICHARD W. ZIOLKOWSKI"));
        assert!(looks_like_author_byline("AND NELSON J. G. FONSECA"));
        assert!(!looks_like_author_byline("A. RUZE LENS"));
        assert!(!looks_like_author_byline("I. INTRODUCTION"));
    }

    #[test]
    fn test_char_weighted_median_font_size_ignores_drop_cap() {
        // Real regression: a single oversized drop-cap glyph ("T") on an
        // otherwise body-sized line used to inflate the line's raw max font
        // size past the heading thresholds. Weighted by character count, the
        // ~50 body-sized characters around it should dominate.
        let blocks = vec![
            text_block_sized(0.0, 0.0, "T", 40.0),
            text_block_sized(20.0, 0.0, "systems are posing challenges to the network", 10.0),
        ];
        let line = vec![0, 1];
        assert_eq!(char_weighted_median_font_size(&blocks, &line), 10.0);
    }

    #[test]
    fn test_char_weighted_median_font_size_keeps_uniform_heading() {
        // A genuinely large-font heading, where all characters share the big
        // size, must still resolve to that size (unchanged from a raw max).
        let blocks = vec![
            text_block_sized(0.0, 0.0, "Introduction", 24.0),
            text_block_sized(150.0, 0.0, "Overview", 24.0),
        ];
        let line = vec![0, 1];
        assert_eq!(char_weighted_median_font_size(&blocks, &line), 24.0);
    }

    #[test]
    fn test_numbered_heading_level_detects_depth() {
        assert_eq!(numbered_heading_level("1 Introduction"), Some("## "));
        assert_eq!(numbered_heading_level("3.2 Attention"), Some("### "));
        assert_eq!(
            numbered_heading_level("3.2.1 Scaled Dot-Product Attention"),
            Some("#### ")
        );
    }

    #[test]
    fn test_numbered_heading_level_rejects_non_headings() {
        // Duration / date-shaped fragments (title not Title-Case-ASCII).
        assert_eq!(numbered_heading_level("3 年"), None);
        // Bare number (empty title).
        assert_eq!(numbered_heading_level("705"), None);
        // Numeric literal, not a section prefix in spirit.
        assert_eq!(numbered_heading_level("1 . 0 · 10 20"), None);
        // Inlined equation fragment (math symbols / mostly single-char tokens).
        assert_eq!(
            numbered_heading_level("1 X m h ∇ θ d log D x m i =1 end for"),
            None
        );
        // Real regression: GAN's "4.1 Global Optimality of p g = p data" —
        // math-polluted title, an accepted residual miss per the design.
        assert_eq!(
            numbered_heading_level("4.1 Global Optimality of p g = p data"),
            None
        );
    }

    #[test]
    fn test_numbered_heading_level_rejects_measurement_sentence_fragments() {
        // Real regressions: a measurement mistaken for a section number,
        // whose "title" is actually a wrapped sentence fragment carrying an
        // interior sentence boundary.
        assert_eq!(
            numbered_heading_level("12.75 GHz. Consequently, it realized a high gain and a wide"),
            None
        );
        assert_eq!(
            numbered_heading_level("100 GHz. Consequently, it is envisioned that 6G will"),
            None
        );
        assert_eq!(
            numbered_heading_level("8 P100 GPUs. Even our base model"),
            None
        );
    }

    #[test]
    fn test_numbered_heading_level_keeps_real_titles_with_no_interior_boundary() {
        assert_eq!(numbered_heading_level("1 Introduction"), Some("## "));
        assert_eq!(numbered_heading_level("3.2 Attention"), Some("### "));
        assert_eq!(
            numbered_heading_level("4.2 Convergence of Algorithm 1"),
            Some("### ")
        );
    }

    #[test]
    fn test_has_interior_sentence_boundary() {
        assert!(has_interior_sentence_boundary("GHz. Consequently, it realized"));
        assert!(has_interior_sentence_boundary("GPUs. Even our base model"));
        assert!(!has_interior_sentence_boundary("Convergence of Algorithm 1"));
        assert!(!has_interior_sentence_boundary("Scaled Dot-Product Attention"));
        // Uppercase-before-dot abbreviation isn't a sentence boundary.
        assert!(!has_interior_sentence_boundary("U.S. Policy"));
    }

    #[test]
    fn test_is_math_font_recognizes_computer_modern_and_amsmath() {
        // Confirmed against the GAN paper's actual (subset-prefixed) font
        // dictionary.
        assert!(is_math_font("ZYPRQG+CMMI10"));
        assert!(is_math_font("ZRBHNJ+CMSY7"));
        assert!(is_math_font("QYOIUU+CMEX10"));
        assert!(is_math_font("XSBJNE+MSBM10"));
        assert!(is_math_font("EFNJYN+CMMIB7"));
    }

    #[test]
    fn test_is_math_font_recognizes_mathtype() {
        // Confirmed against the antenna-review paper's actual font dictionary.
        assert!(is_math_font("RMTMI"));
        assert!(is_math_font("RMTMIB"));
        assert!(is_math_font("MTSYN"));
        assert!(is_math_font("MTSYB"));
        assert!(is_math_font("MTEX"));
    }

    #[test]
    fn test_is_math_font_rejects_body_and_bold_text_fonts() {
        assert!(!is_math_font("NimbusRomNo9L-Regu"));
        assert!(!is_math_font("Times-Roman"));
        assert!(!is_math_font("TTZAKG+CMBX10")); // Computer Modern Bold Extended: bold text, not math
        assert!(!is_math_font("TimesLTStd-Italic"));
    }

    #[test]
    fn test_block_is_math_falls_back_to_symbol_density() {
        // No recognized math font name, but text is dense in math symbols —
        // still classified as math (e.g. a formula typeset without a
        // dedicated math font subset).
        let mut block = text_block(0.0, 0.0, "∑ x ∈ X ∇ θ ≤ ∞");
        block.font_name = "Times-Roman".to_string();
        assert!(block_is_math(&block));

        // Ordinary prose with an incidental symbol or two isn't math.
        let mut prose = text_block(0.0, 0.0, "the temperature rose 4×6 degrees today");
        prose.font_name = "Times-Roman".to_string();
        assert!(!block_is_math(&prose));
    }

    #[test]
    fn test_render_line_with_inline_math_wraps_only_math_runs() {
        let mut var_x = text_block(0.0, 0.0, "the vector x");
        var_x.x_end = 90.0;
        let mut math_run = text_block(95.0, 0.0, "∇θ log D(x)");
        math_run.font_name = "ZYPRQG+CMMI10".to_string();
        math_run.x_end = 200.0;
        let mut tail = text_block(205.0, 0.0, "is the gradient.");
        tail.x_end = 300.0;
        let blocks = vec![var_x, math_run, tail];
        let line = vec![0usize, 1, 2];
        assert_eq!(
            render_line_with_inline_math(&blocks, &line),
            "the vector x $∇θ log D(x)$ is the gradient."
        );
    }

    #[test]
    fn test_render_line_with_inline_math_matches_plain_join_when_no_math() {
        let blocks = vec![text_block(0.0, 0.0, "Hello"), text_block(60.0, 0.0, "World")];
        let line = vec![0usize, 1];
        assert_eq!(render_line_with_inline_math(&blocks, &line), "Hello World");
    }

    #[test]
    fn test_line_math_ratio_weights_by_char_count() {
        let mut math_block = text_block(0.0, 0.0, "min max V(D,G)");
        math_block.font_name = "ZYPRQG+CMMI10".to_string();
        let label = text_block(150.0, 0.0, "G");
        let blocks = vec![math_block, label];
        let line = vec![0usize, 1];
        assert!(line_math_ratio(&blocks, &line) >= DISPLAY_MATH_MIN_RATIO);
    }

    #[test]
    fn test_split_trailing_eq_number_extracts_parenthesized_number() {
        assert_eq!(
            split_trailing_eq_number("E = mc^2 (3)"),
            ("E = mc^2", Some("(3)"))
        );
        assert_eq!(
            split_trailing_eq_number("min max V(D,G) (1.2)"),
            ("min max V(D,G)", Some("(1.2)"))
        );
    }

    #[test]
    fn test_split_trailing_eq_number_no_trailing_parens_is_noop() {
        assert_eq!(
            split_trailing_eq_number("no trailing parens here"),
            ("no trailing parens here", None)
        );
    }

    #[test]
    fn test_split_trailing_eq_number_cannot_distinguish_citation_year() {
        // Known limitation: a trailing all-digit parenthetical is split off
        // as if it were an equation number even when it's actually a
        // citation year — the digits-only shape is identical either way, and
        // this helper only ever runs on lines already gated as
        // display-math-dominant (`DISPLAY_MATH_MIN_RATIO`), so an ordinary
        // citation sentence never reaches it in practice.
        assert_eq!(
            split_trailing_eq_number("as shown by GoodFellow (2014)"),
            ("as shown by GoodFellow", Some("(2014)"))
        );
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
            font_name: String::new(),
        }
    }

    /// Like `text_block`, but with an explicit font size — used by tests that
    /// exercise font-size-dependent behavior (heading detection,
    /// `document_body_size`).
    fn text_block_sized(x: f32, y: f32, text: &str, font_size: f32) -> TextBlock {
        TextBlock {
            font_size,
            ..text_block(x, y, text)
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
        let regions = detect_table_regions(&rows, &line_ys, &[], &[], 12.0);
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
        assert!(detect_table_regions(&single_col, &ys, &[], &[], 12.0).is_empty());

        // A 2-line key:value block (2 aligned columns, but only 2 rows) is
        // exactly the incidental-alignment case MIN_CORE_ROWS guards against.
        let key_value = vec![
            aligned_row(&[(0.0, "Name:"), (100.0, "Alice")]),
            aligned_row(&[(0.0, "Role:"), (100.0, "Engineer")]),
        ];
        let ys2 = vec![300.0, 280.0];
        assert!(detect_table_regions(&key_value, &ys2, &[], &[], 12.0).is_empty());
    }

    #[test]
    fn test_detect_table_regions_bordered_grid_accepts_two_rows() {
        // Same 2-row "key:value"-shaped rows as the conservative-gate case
        // above (which stays empty when borderless), but here bounded by a
        // real ruled box: a rule just above the first row, one just below
        // the last, and an interior vertical divider between the two
        // columns — the independent structural evidence `is_bordered_grid`
        // accepts in place of `MIN_CORE_ROWS`'s third aligned row. Modeled
        // on a genuinely short bordered table (2-3 data rows), the shape
        // seen in the GAN paper's results table.
        let rows = vec![
            aligned_row(&[(0.0, "Name:"), (100.0, "Alice")]),
            aligned_row(&[(0.0, "Role:"), (100.0, "Engineer")]),
        ];
        let ys = vec![300.0, 280.0];
        let h_rules = vec![305.0, 275.0]; // just above row 0, just below row 1
        let v_rules = vec![50.0]; // interior divider between columns at 0.0 and 100.0
        let regions = detect_table_regions(&rows, &ys, &h_rules, &v_rules, 12.0);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].logical_rows.len(), 2);
    }

    #[test]
    fn test_detect_table_regions_bordered_grid_requires_interior_v_rule() {
        // Top and bottom rules alone (no interior vertical divider) aren't
        // enough — a horizontal rule above/below a 2-line paragraph pair is
        // plausible without it being a table at all.
        let rows = vec![
            aligned_row(&[(0.0, "Name:"), (100.0, "Alice")]),
            aligned_row(&[(0.0, "Role:"), (100.0, "Engineer")]),
        ];
        let ys = vec![300.0, 280.0];
        let h_rules = vec![305.0, 275.0];
        assert!(detect_table_regions(&rows, &ys, &h_rules, &[], 12.0).is_empty());
    }

    #[test]
    fn test_is_bordered_grid_requires_all_three_signals() {
        let columns = vec![0.0, 100.0];
        let tol = 12.0;
        // All three present: accepted.
        assert!(is_bordered_grid(300.0, 280.0, &columns, &[305.0, 275.0], &[50.0], tol));
        // Missing top rule.
        assert!(!is_bordered_grid(300.0, 280.0, &columns, &[275.0], &[50.0], tol));
        // Missing bottom rule.
        assert!(!is_bordered_grid(300.0, 280.0, &columns, &[305.0], &[50.0], tol));
        // Missing interior vertical rule.
        assert!(!is_bordered_grid(300.0, 280.0, &columns, &[305.0, 275.0], &[], tol));
        // Fewer than 2 columns: never a grid.
        assert!(!is_bordered_grid(300.0, 280.0, &[0.0], &[305.0, 275.0], &[50.0], tol));
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
        assert!(detect_table_regions(&toc, &ys, &[], &[], 12.0).is_empty());
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
        let regions = detect_table_regions(&rows, &line_ys, &[], &[], 12.0);
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

    #[test]
    fn test_pdfium_lib_path_honors_env_override() {
        // A real, existing file so pdfium_lib_path's `.exists()` check passes
        // and it's picked ahead of every other candidate.
        let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        std::env::set_var("PDFIUM_LIBRARY_PATH", &lib);
        assert_eq!(pdfium_lib_path(), lib);
        std::env::remove_var("PDFIUM_LIBRARY_PATH");
    }

    #[test]
    fn test_pdfium_candidate_paths_includes_resolved_static() {
        // set_pdfium_lib_path (called from main.rs's setup() on Windows) must
        // land in the candidate list, since pdfium_load_diagnostics reports
        // this list verbatim and pdfium_lib_path relies on it being tried
        // before the exe-relative guesses. PDFIUM_RESOLVED_PATH is a
        // process-global OnceLock, so this only asserts presence rather than
        // list position/uniqueness, to stay robust if another test in this
        // binary sets it first.
        let resolved = PathBuf::from("/tmp/pourdown-test-resolved-pdfium.dll");
        set_pdfium_lib_path(resolved.clone());
        assert!(pdfium_candidate_paths().contains(&resolved));
    }

    #[test]
    fn test_diagnose_load_error_hint_missing_file() {
        // .exists() lies (per the caller's own check) take priority over the
        // raw error text entirely — a missing file's raw error is generic.
        let hint = diagnose_load_error_hint("anything", false);
        assert!(hint.contains("bundling/resource issue"));
    }

    #[test]
    fn test_diagnose_load_error_hint_wrong_architecture() {
        let hint = diagnose_load_error_hint(
            "LoadLibraryError(LoadLibraryExW { source: 193 })",
            true,
        );
        assert!(hint.contains("wrong architecture"));
    }

    #[test]
    fn test_diagnose_load_error_hint_missing_symbol_windows() {
        // The real-world Windows text this regression is about: GetProcAddress
        // failing with error 127 (ERROR_PROC_NOT_FOUND) — a missing *export*,
        // not a missing dependency — must NOT get the VCRUNTIME140 hint.
        let hint = diagnose_load_error_hint(
            "LoadLibraryError(GetProcAddress { source: 127 })",
            true,
        );
        assert!(hint.contains("bundled PDFium is an older build"));
        assert!(!hint.contains("VCRUNTIME"));
    }

    #[test]
    fn test_diagnose_load_error_hint_missing_symbol_macos() {
        let hint = diagnose_load_error_hint(
            "LoadLibraryError(DlSym { source: \"dlsym(0x7e32cae0, FPDFTextObj_SetFontSize): symbol not found\" })",
            true,
        );
        assert!(hint.contains("bundled PDFium is an older build"));
    }

    #[test]
    fn test_diagnose_load_error_hint_falls_back_to_missing_dependency() {
        // A genuine missing-dependency load failure (e.g. Windows error 126,
        // "the specified module could not be found") still gets the
        // VCRUNTIME140 hint — this must stay reachable, not just the new
        // missing-symbol branch.
        let hint = diagnose_load_error_hint(
            "LoadLibraryError(LoadLibraryExW { source: 126 })",
            true,
        );
        assert!(hint.contains("VCRUNTIME140"));
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

    #[test]
    fn test_append_wrapped_dehyphenates_lowercase_continuation() {
        let mut acc = String::from("bet-");
        append_wrapped(&mut acc, "ter");
        assert_eq!(acc, "better");
    }

    #[test]
    fn test_append_wrapped_joins_with_space_when_no_hyphen() {
        let mut acc = String::from("goal");
        append_wrapped(&mut acc, "directed");
        assert_eq!(acc, "goal directed");
    }

    #[test]
    fn test_append_wrapped_keeps_hyphen_before_uppercase() {
        // "Multi-" followed by a capitalized continuation is more likely a
        // genuine compound (or the hyphen wasn't actually a wrap split) than
        // a word broken mid-token, so it's left alone rather than merged.
        let mut acc = String::from("Multi-");
        append_wrapped(&mut acc, "Agent");
        assert_eq!(acc, "Multi- Agent");
    }

    #[test]
    fn test_is_list_marker_recognizes_bullets_and_numbers() {
        assert!(is_list_marker("• First point"));
        assert!(is_list_marker("- Dash bullet"));
        assert!(is_list_marker("1. First step"));
        assert!(is_list_marker("IV. Section heading style"));
        assert!(!is_list_marker("This is ordinary prose."));
        assert!(!is_list_marker("A sentence with. a period inside"));
    }

    #[test]
    fn test_detect_gutter_finds_split_for_two_column_layout() {
        let mut blocks = Vec::new();
        for row in 0..6 {
            let y = 500.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "Left column text here"));
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        let gutter = detect_gutter(&blocks);
        assert!(gutter.is_some(), "expected a gutter to be detected");
        let g = gutter.unwrap();
        assert!(
            (100.0..250.0).contains(&g),
            "gutter {g} not between the two columns"
        );
    }

    #[test]
    fn test_detect_gutter_not_diluted_by_one_sided_figure_lines() {
        // Regression test for a real-world figure-dense two-column page (the
        // "METASURFACE-BASED TRANSMITARRAYS" section of an IEEE-style
        // journal PDF) that was misdetected as single-column: a run of
        // figures/captions occupying the left column's height while prose
        // kept flowing on the right produces many lines with content on the
        // right only. Before the fix, MIN_SIDE_SHARE's left/right tally
        // counted blocks from *every* line (including these one-sided
        // ones), so the left share dropped below the 0.2 threshold even
        // though the genuinely two-sided lines were perfectly balanced and
        // full-coverage. The fix restricts the tally to confirmed two-sided
        // lines only.
        let mut blocks = Vec::new();
        for row in 0..6 {
            let y = 500.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "Left column text here"));
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        // Simulates a figure spanning the left column's height while the
        // right column's prose keeps going — no left-side text block at
        // these line heights at all.
        for row in 6..26 {
            let y = 500.0 - row as f32 * 20.0;
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        let gutter = detect_gutter(&blocks);
        assert!(
            gutter.is_some(),
            "a figure-dense two-column page must not be diluted into a false single-column read"
        );
        let g = gutter.unwrap();
        assert!(
            (100.0..250.0).contains(&g),
            "gutter {g} not between the two columns"
        );
    }

    #[test]
    fn test_detect_gutter_returns_none_for_single_column_layout() {
        let mut blocks = Vec::new();
        for row in 0..8 {
            let y = 500.0 - row as f32 * 20.0;
            blocks.push(text_block(
                0.0,
                y,
                "A full width line of body text spanning the page",
            ));
        }
        assert!(detect_gutter(&blocks).is_none());
    }

    #[test]
    fn test_detect_gutter_ignores_dot_leader_toc_lines() {
        // Regression test for a real-world PDF whose TOC page was
        // misdetected as two-column: pdfium extracts each "." of a literal
        // dot-leader as its own tiny text run rather than one merged block,
        // so no single block straddles a candidate gutter — but the line
        // still has title text near the left edge and a page number near
        // the right edge, which otherwise satisfies `detect_gutter`'s
        // two-sided-line evidence exactly like a genuine two-column body
        // would. Each simulated line below is built the same way pdfium
        // would emit it: one block for the title, then many single-"."
        // blocks spaced out across the line, then one block for the page
        // number.
        let mut blocks = Vec::new();
        for row in 0..6 {
            let y = 500.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "13.1 SEARCH AND DISPLAY CONTRACT"));
            for dot in 0..20 {
                blocks.push(text_block(300.0 + dot as f32 * 8.0, y, "."));
            }
            blocks.push(text_block(480.0, y, "65"));
        }
        assert!(
            detect_gutter(&blocks).is_none(),
            "a dot-leader TOC page must not be misread as two-column layout"
        );
    }

    #[test]
    fn test_detect_gutter_finds_split_with_full_width_heading_present() {
        // A full-width heading line plus a two-column body below it — the
        // heading straddles every candidate gutter, but shouldn't prevent
        // detection since the body still dominates the page.
        let mut blocks = vec![text_block(
            0.0,
            600.0,
            "A full width line of body text spanning the page",
        )];
        for row in 0..6 {
            let y = 580.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "Left column text here"));
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        assert!(detect_gutter(&blocks).is_some());
    }

    #[test]
    fn test_segment_page_emits_full_width_region_before_two_column_band() {
        let mut blocks = vec![text_block(
            0.0,
            600.0,
            "A full width line of body text spanning the page",
        )];
        for row in 0..3 {
            let y = 580.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "Left column text here"));
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        let regions = segment_page(&blocks, 235.0, 12.0);
        assert_eq!(regions.len(), 2);
        match &regions[0] {
            Region::Full(indices) => assert_eq!(indices, &vec![0]),
            other => panic!("expected full-width region first, got {other:?}"),
        }
        match &regions[1] {
            Region::TwoCol { left, right } => {
                assert_eq!(left.len(), 3);
                assert_eq!(right.len(), 3);
            }
            other => panic!("expected two-column region second, got {other:?}"),
        }
    }

    #[test]
    fn test_segment_page_treats_split_heading_run_as_full_width() {
        // Regression test for the IEEE Access first-page bug: a heading label
        // ("INDEX TERMS ") and its immediately-following full-width text are
        // two separate runs that individually don't straddle the gutter
        // margin, but sit close enough together (an ordinary run-boundary
        // gap, not a real column gutter) that the line should still be
        // read as one full-width line, not split across the two-column band.
        let gutter = 235.0;
        let mut blocks = vec![
            TextBlock {
                x: 0.0,
                x_end: 230.0,
                y: 600.0,
                font_size: 12.0,
                text: "INDEX TERMS ".to_string(),
                is_image: false,
                font_name: String::new(),
            },
            TextBlock {
                x: 238.0,
                x_end: 400.0,
                y: 600.0,
                font_size: 12.0,
                text: "Agentic AI, autonomous systems, adaptability".to_string(),
                is_image: false,
                font_name: String::new(),
            },
        ];
        for row in 0..3 {
            let y = 580.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "Left column text here"));
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        let regions = segment_page(&blocks, gutter, 12.0);
        assert_eq!(regions.len(), 2);
        match &regions[0] {
            Region::Full(indices) => assert_eq!(indices, &vec![0, 1]),
            other => panic!("expected split heading line as one full-width region, got {other:?}"),
        }
        match &regions[1] {
            Region::TwoCol { left, right } => {
                assert_eq!(left.len(), 3);
                assert_eq!(right.len(), 3);
            }
            other => panic!("expected two-column region second, got {other:?}"),
        }
    }

    #[test]
    fn test_segment_page_keeps_one_sided_line_bucketed_into_open_band() {
        // A line with content on only one side of the gutter (common when a
        // paragraph's last line in one column is shorter than its neighbor,
        // so there's no corresponding text at that height in the other
        // column) must stay bucketed into the open two-column band rather
        // than being forced into its own full-width region — that would
        // fragment ordinary two-column body text into many spurious regions.
        let mut blocks = Vec::new();
        for row in 0..3 {
            let y = 580.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, "Left column text here"));
            blocks.push(text_block(250.0, y, "Right column text here"));
        }
        // A short trailing line with content only on the left.
        blocks.push(text_block(0.0, 520.0, "Left"));

        let regions = segment_page(&blocks, 235.0, 12.0);
        assert_eq!(regions.len(), 1);
        match &regions[0] {
            Region::TwoCol { left, right } => {
                assert_eq!(left.len(), 4);
                assert_eq!(right.len(), 3);
            }
            other => panic!("expected single two-column region, got {other:?}"),
        }
    }

    #[test]
    fn test_two_column_page_does_not_become_table_and_preserves_reading_order() {
        // This is the IEEE Access regression case: a two-column body that,
        // before gutter detection, would satisfy `detect_table_regions`'s
        // alignment gates (each visual "line" merges a left- and
        // right-column line into two aligned cells) and get misrendered as
        // a GFM table with scrambled reading order.
        let mut blocks = Vec::new();
        let left_lines = [
            "Left line one",
            "Left line two",
            "Left line three",
            "Left line four",
        ];
        let right_lines = [
            "Right line one",
            "Right line two",
            "Right line three",
            "Right line four",
        ];
        for (row, (l, r)) in left_lines.iter().zip(right_lines.iter()).enumerate() {
            let y = 500.0 - row as f32 * 20.0;
            blocks.push(text_block(0.0, y, l));
            blocks.push(text_block(250.0, y, r));
        }
        let body_size = 12.0;
        let gutter = detect_gutter(&blocks).expect("should detect two columns");
        let h_rules: Vec<f32> = Vec::new();
        let v_rules: Vec<f32> = Vec::new();
        let mut out = String::new();
        for region in segment_page(&blocks, gutter, body_size) {
            match region {
                Region::Full(indices) => {
                    out.push_str(&render_region(&blocks, &indices, body_size, body_size, &h_rules, &v_rules));
                }
                Region::TwoCol { left, right } => {
                    out.push_str(&render_region(&blocks, &left, body_size, body_size, &h_rules, &v_rules));
                    out.push_str(&render_region(&blocks, &right, body_size, body_size, &h_rules, &v_rules));
                }
            }
        }

        assert!(
            !out.contains("| --- |"),
            "two-column prose should not become a table:\n{out}"
        );
        let left_pos = out.find("Left line one").expect("left column text missing");
        let right_pos = out
            .find("Right line one")
            .expect("right column text missing");
        assert!(
            left_pos < right_pos,
            "left column should be fully emitted before right column:\n{out}"
        );
    }

    #[test]
    fn test_normalize_hf_collapses_digit_runs_and_case() {
        assert_eq!(normalize_hf("18913"), "#");
        assert_eq!(normalize_hf("VOLUME 13, 2025"), "volume #, #");
        assert_eq!(
            normalize_hf("  D.  B. Acharya  et al. "),
            "d. b. acharya et al."
        );
        // Two separate digit runs on one line each collapse to their own '#'.
        assert_eq!(normalize_hf("page 4 of 20"), "page # of #");
    }

    /// Builds a `PageContent` with a running header near the top of the page
    /// (y=780, within the top band for height=800) and a page-number-style
    /// footer near the bottom (y=20, within the bottom band), plus one line
    /// of ordinary body text in the middle (y=400, outside both bands).
    fn page_with_header_footer(header: &str, footer: &str, body: &str) -> PageContent {
        PageContent {
            blocks: vec![
                text_block(50.0, 780.0, header),
                text_block(50.0, 400.0, body),
                text_block(50.0, 20.0, footer),
            ],
            h_rules: Vec::new(),
            v_rules: Vec::new(),
            height: 800.0,
        }
    }

    #[test]
    fn test_detect_running_headers_footers_finds_band_repeats_across_pages() {
        let pages = vec![
            page_with_header_footer("D. B. Acharya et al.: Survey", "18913", "Body text one"),
            page_with_header_footer("D. B. Acharya et al.: Survey", "18914", "Body text two"),
            page_with_header_footer("D. B. Acharya et al.: Survey", "18915", "Body text three"),
            page_with_header_footer("D. B. Acharya et al.: Survey", "18916", "Body text four"),
        ];
        let keys = detect_running_headers_footers(&pages);

        assert!(
            keys.contains(&normalize_hf("D. B. Acharya et al.: Survey")),
            "expected running header to be detected: {keys:?}"
        );
        assert!(
            keys.contains(&normalize_hf("18913")),
            "expected digit-normalized page number to be detected: {keys:?}"
        );
        // Body-position text never enters the band, so it must not be
        // treated as a running header/footer even though it "recurs" (each
        // page's body text is distinct here, but the position check alone
        // should already exclude it).
        assert!(!keys.contains(&normalize_hf("Body text one")));
    }

    #[test]
    fn test_detect_running_headers_footers_requires_min_pages() {
        // Only 2 pages share the header text — below HF_MIN_PAGES (3).
        let pages = vec![
            page_with_header_footer("Rare Header", "1", "Body A"),
            page_with_header_footer("Rare Header", "2", "Body B"),
        ];
        let keys = detect_running_headers_footers(&pages);
        assert!(
            !keys.contains(&normalize_hf("Rare Header")),
            "a header repeated on fewer than HF_MIN_PAGES pages should not be flagged"
        );
    }

    #[test]
    fn test_filter_header_footer_blocks_removes_only_flagged_lines() {
        let page = page_with_header_footer("D. B. Acharya et al.: Survey", "18913", "Body text");
        let mut hf_keys = HashSet::new();
        hf_keys.insert(normalize_hf("D. B. Acharya et al.: Survey"));
        hf_keys.insert(normalize_hf("18913"));

        let kept = filter_header_footer_blocks(&page.blocks, page.height, &hf_keys);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "Body text");
    }

    #[test]
    fn test_filter_header_footer_blocks_noop_when_no_keys() {
        let page = page_with_header_footer("D. B. Acharya et al.: Survey", "18913", "Body text");
        let kept = filter_header_footer_blocks(&page.blocks, page.height, &HashSet::new());
        assert_eq!(kept.len(), page.blocks.len());
    }

    // --- TOC region detection / rendering ---

    #[test]
    fn test_has_leader_recognizes_glyph_and_dots() {
        assert!(has_leader("Introduction … 5"));
        assert!(has_leader("Introduction .... 5"));
        assert!(!has_leader("Ordinary prose."));
    }

    #[test]
    fn test_starts_with_section_number_accepts_toc_prefixes() {
        assert!(starts_with_section_number("13. G – CONTRACT"));
        assert!(starts_with_section_number("13.1 G01 – SEARCH"));
        assert!(starts_with_section_number("13.1.2 Sub-entry"));
    }

    #[test]
    fn test_starts_with_section_number_rejects_dates_and_letter_prefixes() {
        // Dash breaks the digit/dot run before any following space.
        assert!(!starts_with_section_number("2024-07-26 Initial Version"));
        // No space after the numeric run.
        assert!(!starts_with_section_number("13.G – CONTRACT"));
        assert!(!starts_with_section_number("Ordinary prose"));
    }

    #[test]
    fn test_is_bare_page_ref() {
        assert!(is_bare_page_ref("… 65"));
        assert!(is_bare_page_ref("...... 68"));
        assert!(!is_bare_page_ref("13.1 G01 … 65")); // has non-digit title text
        assert!(!is_bare_page_ref("…")); // no digits at all
    }

    #[test]
    fn test_ends_with_page_number_and_trailing_digits() {
        assert!(ends_with_page_number("13.1 G01 – SEARCH … 65"));
        assert!(!ends_with_page_number("13.1 G01 – SEARCH …"));
        assert_eq!(trailing_digits("13.1 G01 – SEARCH … 65"), "65");
        assert_eq!(trailing_digits("no digits here"), "");
    }

    #[test]
    fn test_content_image_key_stable_for_identical_bytes_distinct_otherwise() {
        let bytes_a = vec![1u8, 2, 3, 4];
        let bytes_b = vec![1u8, 2, 3, 4];
        let bytes_c = vec![9u8, 9, 9];
        assert_eq!(content_image_key(&bytes_a), content_image_key(&bytes_b));
        assert_ne!(content_image_key(&bytes_a), content_image_key(&bytes_c));
    }

    #[test]
    fn test_detect_toc_regions_finds_run_with_enough_leaders() {
        let lines: Vec<String> = vec![
            "13. G – CONTRACT … 65".to_string(),
            "13.1 G01 – SEARCH & DISPLAY CONTRACT …".to_string(),
            "13.2 G02 – MAINTAIN CONTRACT RECORDS …".to_string(),
            "… 65".to_string(),
            "… 68".to_string(),
        ];
        let is_image = vec![false; lines.len()];
        let regions = detect_toc_regions(&lines, &is_image);
        assert_eq!(regions, vec![(0, 4)]);
    }

    #[test]
    fn test_detect_toc_regions_ignores_isolated_leader_line() {
        // Only one leader line in the whole page — below TOC_MIN_LEADER_LINES,
        // so it's left to the existing per-line fallback instead of being
        // swept into region treatment.
        let lines: Vec<String> = vec![
            "Ordinary paragraph text.".to_string(),
            "See figure below ..... 5".to_string(),
            "More ordinary prose follows here.".to_string(),
        ];
        let is_image = vec![false; lines.len()];
        assert!(detect_toc_regions(&lines, &is_image).is_empty());
    }

    #[test]
    fn test_render_toc_region_reassociates_detached_page_numbers() {
        // Regression test for the reported bug: titles stream in, then their
        // page numbers arrive as separate detached lines afterward (FIFO
        // order), and a wrapped title tail should merge into its entry.
        let lines: Vec<String> = vec![
            "14. H – DOCTOR APPROVAL … 75".to_string(),
            "14.1 H01 – MAINTAIN APPROVAL LABEL IN SERVICE LEVEL … 76".to_string(),
            "14.2 H02 – APPROVAL LABEL (WITH SERVICE) ON NEW CS CODE /".to_string(),
            "CS CODE VERSION … 82".to_string(),
        ];
        let is_image = vec![false; lines.len()];
        let md = render_toc_region(&lines, &is_image, 0, lines.len() - 1);

        assert!(!md.contains('#'), "TOC region must never emit a heading:\n{md}");
        assert!(
            md.contains("- 14.2 H02 – APPROVAL LABEL (WITH SERVICE) ON NEW CS CODE / CS CODE VERSION … 82"),
            "expected wrapped title tail merged into its entry:\n{md}"
        );
    }

    #[test]
    fn test_render_toc_region_fifo_assigns_detached_page_numbers_in_order() {
        let lines: Vec<String> = vec![
            "13.1 G01 – SEARCH & DISPLAY CONTRACT …".to_string(),
            "13.2 G02 – MAINTAIN CONTRACT RECORDS …".to_string(),
            "… 65".to_string(),
            "… 68".to_string(),
        ];
        let is_image = vec![false; lines.len()];
        let md = render_toc_region(&lines, &is_image, 0, lines.len() - 1);
        let bullets: Vec<&str> = md.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(bullets.len(), 2);
        assert!(bullets[0].ends_with("65"), "first entry should get 65:\n{md}");
        assert!(bullets[1].ends_with("68"), "second entry should get 68:\n{md}");
    }

    // --- Figure/table caption association ---

    #[test]
    fn test_is_caption_label_recognizes_labels_with_trailing_numeral() {
        assert!(is_caption_label("Figure 3: Model architecture"));
        assert!(is_caption_label("Fig. 2 Loss curves"));
        assert!(is_caption_label("Table 1. Results"));
        assert!(is_caption_label("表 1 結果"));
        assert!(is_caption_label("圖 2 模型架構"));
    }

    #[test]
    fn test_is_caption_label_recognizes_all_caps_ieee_style() {
        // Confirmed against a real IEEE-style paper's actual caption text.
        assert!(is_caption_label(
            "FIGURE 1. Schematic representation of a Ruze lens with constant thickness."
        ));
        assert!(is_caption_label("TABLE 1. Comparison of traditional AI and agentic AI."));
    }

    #[test]
    fn test_is_caption_label_rejects_label_word_without_numeral() {
        assert!(!is_caption_label("Table tennis is popular"));
        assert!(!is_caption_label("Figures like this one are common"));
        assert!(!is_caption_label("Ordinary prose"));
    }

    #[test]
    fn test_with_caption_alt_prefers_following_line() {
        let line_texts = vec![
            "![](assets/fig.png)".to_string(),
            "Figure 3: Model architecture".to_string(),
        ];
        assert_eq!(
            with_caption_alt(&line_texts[0], &line_texts, 0),
            "![Figure 3: Model architecture](assets/fig.png)"
        );
    }

    #[test]
    fn test_with_caption_alt_falls_back_to_preceding_line() {
        let line_texts = vec![
            "Table 2: Ablation study".to_string(),
            "![](assets/tab.png)".to_string(),
        ];
        assert_eq!(
            with_caption_alt(&line_texts[1], &line_texts, 1),
            "![Table 2: Ablation study](assets/tab.png)"
        );
    }

    #[test]
    fn test_with_caption_alt_noop_without_adjacent_caption() {
        let line_texts = vec![
            "Some unrelated paragraph.".to_string(),
            "![](assets/fig.png)".to_string(),
            "Another unrelated paragraph.".to_string(),
        ];
        assert_eq!(
            with_caption_alt(&line_texts[1], &line_texts, 1),
            "![](assets/fig.png)"
        );
    }

    #[test]
    fn test_with_caption_alt_noop_for_unsupported_image_placeholder() {
        let line_texts = vec![
            "*(unsupported image)*".to_string(),
            "Figure 4: Vector diagram".to_string(),
        ];
        assert_eq!(
            with_caption_alt(&line_texts[0], &line_texts, 0),
            "*(unsupported image)*"
        );
    }

    // --- Repeated image stripping ---

    /// Builds an image `TextBlock` (as `extract_page_blocks` would) with the
    /// given already-rendered link text.
    fn image_block(x: f32, y: f32, text: &str) -> TextBlock {
        TextBlock {
            x,
            x_end: x,
            y,
            font_size: 0.0,
            text: text.to_string(),
            is_image: true,
            font_name: String::new(),
        }
    }

    fn page_with_logo(logo_text: &str) -> PageContent {
        PageContent {
            blocks: vec![
                image_block(50.0, 780.0, logo_text),
                text_block(50.0, 400.0, "Unique body text for this page"),
            ],
            h_rules: Vec::new(),
            v_rules: Vec::new(),
            height: 800.0,
        }
    }

    #[test]
    fn test_detect_repeated_images_flags_logo_recurring_across_pages() {
        let pages = vec![
            page_with_logo("![](assets/image1.png)"),
            page_with_logo("![](assets/image1.png)"),
            page_with_logo("![](assets/image1.png)"),
        ];
        let repeated = detect_repeated_images(&pages);
        assert!(repeated.contains("![](assets/image1.png)"));
    }

    #[test]
    fn test_detect_repeated_images_requires_min_pages() {
        let pages = vec![
            page_with_logo("![](assets/image1.png)"),
            page_with_logo("![](assets/image1.png)"),
        ];
        let repeated = detect_repeated_images(&pages);
        assert!(
            repeated.is_empty(),
            "an image on fewer than HF_MIN_PAGES pages should not be flagged"
        );
    }

    #[test]
    fn test_filter_repeated_images_removes_only_flagged_images_keeps_text() {
        let page = page_with_logo("![](assets/image1.png)");
        let mut repeated = HashSet::new();
        repeated.insert("![](assets/image1.png)".to_string());

        let kept = filter_repeated_images(&page.blocks, &repeated);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "Unique body text for this page");
    }

    #[test]
    fn test_filter_repeated_images_noop_when_empty() {
        let page = page_with_logo("![](assets/image1.png)");
        let kept = filter_repeated_images(&page.blocks, &HashSet::new());
        assert_eq!(kept.len(), page.blocks.len());
    }

    #[test]
    fn test_document_body_size_ignores_page_dense_with_small_runs_and_images() {
        // Simulates a reference-/equation-/caption-heavy page: lots of short
        // small-font runs plus a zero-size image block, alongside a page of
        // ordinary 12pt prose. A per-page median (like `render_page_blocks`
        // computes) would be dragged down by the dense page; the document-wide
        // baseline should still land on 12pt because that's where the bulk of
        // the document's *characters* are.
        let mut dense_page_blocks: Vec<TextBlock> = (0..30)
            .map(|i| text_block_sized(0.0, 700.0 - i as f32 * 5.0, "x", 6.0))
            .collect();
        dense_page_blocks.push(TextBlock {
            font_size: 0.0,
            is_image: true,
            ..text_block(0.0, 780.0, "![](assets/fig.png)")
        });
        let dense_page = PageContent {
            blocks: dense_page_blocks,
            h_rules: Vec::new(),
            v_rules: Vec::new(),
            height: 800.0,
        };

        let prose_page = PageContent {
            blocks: vec![text_block_sized(
                0.0,
                700.0,
                "This is a long sentence of ordinary body text at the document's true body font size",
                12.0,
            )],
            h_rules: Vec::new(),
            v_rules: Vec::new(),
            height: 800.0,
        };

        let pages = vec![dense_page, prose_page];
        let size = document_body_size(&pages);
        assert_eq!(size, 12.0, "should anchor on the dominant body text, not the dense page's small runs");
    }

    #[test]
    fn test_document_body_size_empty_pages_returns_zero() {
        assert_eq!(document_body_size(&[]), 0.0);
    }

    #[test]
    fn test_render_region_does_not_promote_long_lowercase_wrap_to_heading() {
        // Regression test for the reported bug: a page/document-baseline
        // mismatch previously let ordinary wrapped body lines clear the
        // font-ratio heading threshold. Even if this line's font technically
        // clears the ratio, its shape (long, lowercase start, no ending
        // punctuation is irrelevant here — length is what should disqualify
        // it) should keep it out of the heading path.
        let blocks = vec![text_block_sized(
            0.0,
            500.0,
            "one can introduce phase correcting elements into the zones as depicted in the figure below and this sentence runs long",
            14.0,
        )];
        let indices = vec![0];
        let body_size = 12.0;
        let heading_body_size = 12.0; // ratio 14/12 ~= 1.17, clears the 1.15 "###" gate
        let out = render_region(&blocks, &indices, body_size, heading_body_size, &[], &[]);
        assert!(
            !out.trim_start().starts_with('#'),
            "an over-length line should not become a heading even if font ratio clears the gate:\n{out}"
        );
    }

    #[test]
    fn test_render_region_large_font_bullet_is_list_not_heading() {
        let blocks = vec![text_block_sized(0.0, 500.0, "• Goal Alignment with Human Values", 16.0)];
        let indices = vec![0];
        let body_size = 12.0;
        let heading_body_size = 12.0;
        let out = render_region(&blocks, &indices, body_size, heading_body_size, &[], &[]);
        assert!(
            out.trim_start().starts_with("• "),
            "a bulleted line should render as a list item, not a heading, regardless of font size:\n{out}"
        );
    }

    #[test]
    fn test_render_region_short_all_caps_still_becomes_heading() {
        let blocks = vec![text_block(0.0, 500.0, "J. ROADMAP FOR FUTURE RESEARCH")];
        let indices = vec![0];
        let body_size = 12.0;
        let heading_body_size = 12.0; // same font size as body: ratio path won't fire
        let out = render_region(&blocks, &indices, body_size, heading_body_size, &[], &[]);
        assert!(
            out.starts_with("## J. ROADMAP FOR FUTURE RESEARCH"),
            "a genuine ALL-CAPS heading should still be promoted:\n{out}"
        );
    }

}
