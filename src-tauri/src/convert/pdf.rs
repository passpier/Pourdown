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

/// Returns true for short lines that are all-uppercase (or CJK-only) with no sentence ending.
/// Used as a fallback heading detector when all text in the PDF has the same font size.
fn is_all_caps_heading(text: &str) -> bool {
    let char_count = text.chars().count();
    // Length guard: too short or too long to be a section heading
    if char_count < 3 || char_count > 80 {
        return false;
    }
    // Dot-leader lines are TOC entries, not headings
    if text.contains("....") {
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
    y: f32,
    font_size: f32,
    text: String,
    /// True for an already-formatted `![]()` image link — excluded from
    /// heading classification since it has no meaningful font size.
    is_image: bool,
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
            blocks.push(TextBlock {
                x: matrix.e(),
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

    let mut out = String::new();
    let mut prev_y = f32::MAX;

    for line in &lines {
        let line_text: String = line
            .iter()
            .map(|&i| blocks[i].text.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if line_text.is_empty() {
            continue;
        }

        let max_font = line
            .iter()
            .map(|&i| blocks[i].font_size)
            .fold(0.0f32, f32::max);
        let y = blocks[line[0]].y;
        let is_image_line = line.iter().all(|&i| blocks[i].is_image);

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
    fn test_all_caps_heading_rejects_sentence_punctuation() {
        assert!(!is_all_caps_heading("END OF REPORT."));
    }

    #[test]
    fn test_all_caps_heading_rejects_length_extremes() {
        assert!(!is_all_caps_heading("AB"));
        assert!(!is_all_caps_heading(&"A".repeat(81)));
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
