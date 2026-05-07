use markdown2pdf::config::ConfigSource;
use pdfium_render::prelude::*;
use std::path::PathBuf;
use std::sync::Mutex;


use super::ConversionError;

// Guards the one-time initialization of the global pdfium bindings.
static PDFIUM_INIT: Mutex<bool> = Mutex::new(false);

const PDF_IMPORT_NOTICE: &str = "> **Import Notice**: This PDF was imported with layout analysis. \
Headings and paragraphs are inferred from font sizes and spacing. \
Images and complex multi-column layouts may not be fully preserved.\n\n";

/// Convert Markdown to a PDF file.
pub fn markdown_to_pdf(markdown: &str, path: &str) -> Result<(), ConversionError> {
    markdown2pdf::parse_into_file(markdown.to_string(), path, ConfigSource::Default, None)
        .map_err(|e| ConversionError(format!("PDF export failed: {}", e)))
}

/// Convert a PDF file to Markdown using layout-aware extraction.
pub fn pdf_to_markdown(path: &str) -> Result<String, ConversionError> {
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
    for page in doc.pages().iter() {
        md.push_str(&extract_page_markdown(&page)?);
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
}

fn extract_page_markdown(page: &PdfPage) -> Result<String, ConversionError> {
    let mut blocks: Vec<TextBlock> = Vec::new();

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

        // Insert blank line on large vertical gap between sections
        if prev_y != f32::MAX && (prev_y - y) > body_size * 2.5 {
            out.push('\n');
        }

        // Classify heading level: first try font size ratio, then ALL-CAPS heuristic
        let heading = if max_font >= body_size * 1.8 {
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
