use std::collections::HashMap;
use super::inline_fmt::{apply_inline_fmt, escape_markdown};
use super::media::MediaSink;
use super::ConversionError;

/// Convert a PPTX file to Markdown.
/// Each slide becomes a section separated by `---`. Embedded images are
/// extracted via `media` and rendered as real `![]()` links (falling back to
/// a text note for non-renderable formats like EMF/WMF).
pub fn pptx_to_markdown(path: &str, media: &mut MediaSink) -> Result<String, ConversionError> {
    use std::io::Read;

    let file = std::fs::File::open(path)
        .map_err(|e| ConversionError(format!("Failed to open PPTX: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ConversionError(format!("Failed to read PPTX archive: {}", e)))?;

    // Collect rels, slide XML, and media bytes in a single pass
    let mut rels_map: HashMap<usize, String> = HashMap::new();
    let mut slides_raw: Vec<(usize, String)> = Vec::new();
    let mut media_bytes: HashMap<String, Vec<u8>> = HashMap::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ConversionError(format!("Failed to read archive entry: {}", e)))?;
        let name = entry.name().to_string();

        if name.starts_with("ppt/slides/_rels/slide") && name.ends_with(".xml.rels") {
            let num: usize = name
                .trim_start_matches("ppt/slides/_rels/slide")
                .trim_end_matches(".xml.rels")
                .parse()
                .unwrap_or(0);
            if num > 0 {
                let mut content = String::new();
                entry
                    .read_to_string(&mut content)
                    .map_err(|e| ConversionError(format!("Failed to read rels: {}", e)))?;
                rels_map.insert(num, content);
            }
        } else if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
            let num: usize = name
                .trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse()
                .unwrap_or(0);
            if num > 0 {
                let mut content = String::new();
                entry
                    .read_to_string(&mut content)
                    .map_err(|e| ConversionError(format!("Failed to read slide XML: {}", e)))?;
                slides_raw.push((num, content));
            }
        } else if name.starts_with("ppt/media/") {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| ConversionError(format!("Failed to read media entry: {}", e)))?;
            media_bytes.insert(name, buf);
        }
    }

    slides_raw.sort_by_key(|(n, _)| *n);

    let mut parts: Vec<String> = Vec::new();
    for (num, xml) in &slides_raw {
        let rels = rels_map
            .get(num)
            .map(|r| parse_slide_rels(r))
            .unwrap_or_default();
        let text = extract_slide_content(xml, &rels, &media_bytes, media);
        if !text.is_empty() {
            parts.push(text);
        }
    }

    Ok(parts.join("\n\n---\n\n"))
}

/// Parse a slide rels XML and return a map of rId → resolved `ppt/media/...`
/// path. `Target` is relative to `ppt/slides/` (e.g. `../media/image1.png`),
/// so it's resolved against that base to get the archive entry name.
fn parse_slide_rels(rels_xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for chunk in rels_xml.split("<Relationship ") {
        if !chunk.contains("/image") {
            continue;
        }
        if let (Some(id), Some(target)) = (get_xml_attr(chunk, "Id"), get_xml_attr(chunk, "Target")) {
            map.insert(id, resolve_slide_relative_path(&target));
        }
    }
    map
}

/// Resolve a path relative to `ppt/slides/` (e.g. `../media/image1.png`)
/// into an absolute-in-archive path (e.g. `ppt/media/image1.png`).
fn resolve_slide_relative_path(target: &str) -> String {
    let mut base: Vec<&str> = vec!["ppt", "slides"];
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

/// Extract the value of a named XML attribute from a fragment.
fn get_xml_attr(s: &str, attr: &str) -> Option<String> {
    let search = format!("{}=\"", attr);
    if let Some(pos) = s.find(&search) {
        let after = &s[pos + search.len()..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    }
    None
}

/// Extract slide content: title (as `# heading`), images, and body paragraphs.
fn extract_slide_content(
    xml: &str,
    rels: &HashMap<String, String>,
    media_bytes: &HashMap<String, Vec<u8>>,
    media: &mut MediaSink,
) -> String {
    // Strip the slide background block so its image embeds aren't treated as content images
    let stripped: String;
    let xml: &str = if let Some(bg_start) = xml.find("<p:bg>").or_else(|| xml.find("<p:bg ")) {
        if let Some(rel_end) = xml[bg_start..].find("</p:bg>") {
            stripped = format!(
                "{}{}",
                &xml[..bg_start],
                &xml[bg_start + rel_end + "</p:bg>".len()..]
            );
            &stripped
        } else {
            xml
        }
    } else {
        xml
    };

    // Collect content images from r:embed references (deduplicated), extracting
    // bytes via `media` into real `![]()` links; non-renderable formats
    // (e.g. EMF/WMF) fall back to a text note instead of a broken image.
    let mut image_placeholders: Vec<String> = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = xml[search_from..].find("r:embed=\"") {
        let abs = search_from + pos + 9;
        if let Some(end) = xml[abs..].find('"') {
            let rid = &xml[abs..abs + end];
            if let Some(media_path) = rels.get(rid) {
                let placeholder = if let Some(bytes) = media_bytes.get(media_path) {
                    match media.add(media_path, bytes) {
                        Some(rel_path) => format!("![]({})", rel_path),
                        None => format!(
                            "*(unsupported image: {})*",
                            media_path.rsplit('/').next().unwrap_or(media_path)
                        ),
                    }
                } else {
                    format!(
                        "*(unsupported image: {})*",
                        media_path.rsplit('/').next().unwrap_or(media_path)
                    )
                };
                if !image_placeholders.contains(&placeholder) {
                    image_placeholders.push(placeholder);
                }
            }
        }
        search_from = abs;
    }

    // Extract title from a placeholder shape (<p:ph type="title"> or "ctrTitle") if present
    let ph_title = find_placeholder_title(xml);

    // Extract all paragraphs from the slide, building the body
    let (fallback_title, body) = extract_paragraphs(xml, ph_title.as_deref());

    let title = ph_title.as_deref().or(fallback_title.as_deref());

    let mut output = String::new();

    if let Some(t) = title {
        if !t.is_empty() {
            output.push_str(&format!("# {}", t));
        }
    }

    for img in &image_placeholders {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(img);
    }

    if !body.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&body);
    }

    output.trim().to_string()
}

/// Look for a `<p:sp>` containing `<p:ph type="title"` or `type="ctrTitle"` and return its text.
fn find_placeholder_title(xml: &str) -> Option<String> {
    let markers = [r#"type="title""#, r#"type="ctrTitle""#];
    for marker in &markers {
        if let Some(marker_pos) = xml.find(marker) {
            // Find the enclosing <p:sp> start by scanning backward
            let prefix = &xml[..marker_pos];
            if let Some(sp_start) = prefix.rfind("<p:sp") {
                // Find the closing </p:sp> after the marker
                if let Some(rel_end) = xml[sp_start..].find("</p:sp>") {
                    let sp_block = &xml[sp_start..sp_start + rel_end + "</p:sp>".len()];
                    let text = collect_runs_text(sp_block);
                    if !text.is_empty() {
                        return Some(text.trim().to_string());
                    }
                }
            }
        }
    }
    None
}

/// Collect all `<a:t>` text from a block, concatenating runs within each paragraph with spaces.
fn collect_runs_text(block: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for para_chunk in block.split("<a:p>").skip(1) {
        let para_end = para_chunk.find("</a:p>").unwrap_or(para_chunk.len());
        let para = &para_chunk[..para_end];
        let text = extract_run_text_plain(para);
        if !text.is_empty() {
            parts.push(text);
        }
    }
    parts.join(" ")
}

/// Extract plain concatenated text from all `<a:t>` tags within a paragraph block.
fn extract_run_text_plain(para: &str) -> String {
    let mut result = String::new();
    for part in para.split("<a:t>").skip(1) {
        if let Some(end) = part.find("</a:t>") {
            result.push_str(&xml_decode(&part[..end]));
        }
    }
    result.trim().to_string()
}

/// Extract paragraphs from ALL shapes on the slide.
/// The `known_title` text is excluded from the body (to avoid duplication).
/// Returns (first_para_as_fallback_title, body_text).
fn extract_paragraphs(xml: &str, known_title: Option<&str>) -> (Option<String>, String) {
    let mut first_para: Option<String> = None;
    let mut body_parts: Vec<(bool, usize, String)> = Vec::new(); // (is_bullet, level, text)

    for para_chunk in xml.split("<a:p>").skip(1) {
        let para_end = para_chunk.find("</a:p>").unwrap_or(para_chunk.len());
        let para = &para_chunk[..para_end];

        // Check bullet: presence of <a:buNone means NOT a bullet
        let has_bu_none = para.contains("<a:buNone");
        let is_bullet = !has_bu_none;

        // Indentation level
        let level = get_xml_attr(para, "lvl")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        // Build run text with inline formatting
        let text = extract_run_text_formatted(para);
        let trimmed = text.trim().to_string();

        if trimmed.is_empty() {
            continue;
        }

        // Skip if this text is the known title (avoid duplication)
        if let Some(kt) = known_title {
            if trimmed == kt {
                continue;
            }
        }

        if known_title.is_none() && first_para.is_none() {
            first_para = Some(trimmed);
        } else {
            body_parts.push((is_bullet, level, trimmed));
        }
    }

    // Build body string with proper Markdown paragraph/bullet separators
    let mut body = String::new();
    let mut last_was_bullet = false;

    for (is_bullet, level, text) in &body_parts {
        if *is_bullet {
            let indent = "  ".repeat(*level);
            let sep = if last_was_bullet { "\n" } else if body.is_empty() { "" } else { "\n\n" };
            body.push_str(&format!("{}{}- {}", sep, indent, text));
            last_was_bullet = true;
        } else {
            let sep = if body.is_empty() { "" } else { "\n\n" };
            body.push_str(&format!("{}{}", sep, text));
            last_was_bullet = false;
        }
    }

    (first_para, body)
}

/// Extract text from a paragraph's runs with basic bold/italic Markdown formatting.
///
/// Runs are first collected into (text, bold, italic) segments, adjacent
/// segments with identical formatting are merged, and only then wrapped in
/// emphasis markers. Wrapping each run independently would produce artifacts
/// like `**專案**` + `**範疇**` = `**專案****範疇**` (invalid/ambiguous
/// CommonMark) whenever a formatting run happens to be split across two
/// `<a:r>` elements. `apply_inline_fmt` additionally keeps leading/trailing
/// whitespace outside the markers (`**Frontline **` is not a valid closer).
fn extract_run_text_formatted(para: &str) -> String {
    let mut segments: Vec<(String, bool, bool)> = Vec::new();
    for run_chunk in para.split("<a:r>").skip(1) {
        let run_end = run_chunk.find("</a:r>").unwrap_or(run_chunk.len());
        let run = &run_chunk[..run_end];

        // Detect bold/italic from <a:rPr> tag
        let (is_bold, is_italic) = if let Some(rpr_start) = run.find("<a:rPr") {
            let rpr_end = run[rpr_start..].find('>').unwrap_or(run.len() - rpr_start);
            let rpr = &run[rpr_start..rpr_start + rpr_end];
            let bold = rpr.contains(" b=\"1\"") || rpr.contains("\tb=\"1\"");
            let italic = rpr.contains(" i=\"1\"") || rpr.contains("\ti=\"1\"");
            (bold, italic)
        } else {
            (false, false)
        };

        // Extract text content
        if let Some(t_start) = run.find("<a:t>") {
            let after = &run[t_start + 5..];
            if let Some(t_end) = after.find("</a:t>") {
                let text = escape_markdown(&xml_decode(&after[..t_end]));
                if !text.is_empty() {
                    segments.push((text, is_bold, is_italic));
                }
            }
        }
    }

    // Merge adjacent segments with identical formatting to prevent `****` artifacts.
    let mut merged: Vec<(String, bool, bool)> = Vec::new();
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if last.1 == seg.1 && last.2 == seg.2 {
                last.0.push_str(&seg.0);
                continue;
            }
        }
        merged.push(seg);
    }

    let mut result = String::new();
    for (text, bold, italic) in merged {
        // pptx runs don't carry strikethrough detection today, so pass false.
        result.push_str(&apply_inline_fmt(&text, bold, italic, false));
    }
    result
}

fn xml_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_decode() {
        assert_eq!(
            xml_decode("A &amp; B &lt;tag&gt; &quot;q&quot; &apos;s&apos;"),
            "A & B <tag> \"q\" 's'"
        );
    }

    #[test]
    fn test_resolve_slide_relative_path() {
        assert_eq!(resolve_slide_relative_path("../media/image1.png"), "ppt/media/image1.png");
    }

    #[test]
    fn test_parse_slide_rels() {
        let xml = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/></Relationships>"#;
        let map = parse_slide_rels(xml);
        assert_eq!(map.get("rId1").unwrap(), "ppt/media/image1.png");
    }

    #[test]
    fn test_find_placeholder_title() {
        let xml = r#"<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Hello</a:t></a:r></a:p></p:txBody></p:sp>"#;
        assert_eq!(find_placeholder_title(xml), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_paragraphs_first_para_then_bullet() {
        let xml = "<a:p><a:r><a:t>First</a:t></a:r></a:p><a:p><a:r><a:t>Second</a:t></a:r></a:p>";
        let (first, body) = extract_paragraphs(xml, None);
        assert_eq!(first, Some("First".to_string()));
        assert_eq!(body, "- Second");
    }

    #[test]
    fn test_extract_run_text_formatted_merges_adjacent_bold_runs() {
        // Regression test: a single bold phrase split across two <a:r> runs
        // (e.g. PowerPoint spell-check re-splitting a run) must not produce
        // `**a****b**` — adjacent same-format segments are merged first.
        let para = concat!(
            r#"<a:r><a:rPr b="1"/><a:t>專案</a:t></a:r>"#,
            r#"<a:r><a:rPr b="1"/><a:t>範疇</a:t></a:r>"#,
        );
        assert_eq!(extract_run_text_formatted(para), "**專案範疇**");
    }

    #[test]
    fn test_extract_run_text_formatted_trailing_space_outside_markers() {
        // `**Frontline **` is not valid CommonMark (closer can't be preceded
        // by whitespace); the trailing space must move outside the markers.
        let para = concat!(
            r#"<a:r><a:rPr b="1"/><a:t>Frontline </a:t></a:r>"#,
            r#"<a:r><a:t>能提供</a:t></a:r>"#,
        );
        assert_eq!(extract_run_text_formatted(para), "**Frontline** 能提供");
    }

    #[test]
    fn test_extract_run_text_formatted_escapes_literal_markdown_chars() {
        let para = r#"<a:r><a:t>* not a bullet</a:t></a:r>"#;
        assert_eq!(extract_run_text_formatted(para), "\\* not a bullet");
    }

    #[test]
    fn test_extract_run_text_formatted_italic() {
        let para = r#"<a:r><a:rPr i="1"/><a:t>hi</a:t></a:r>"#;
        assert_eq!(extract_run_text_formatted(para), "*hi*");
    }

    /// End-to-end regression test against `tests/fixtures/sample.pptx`
    /// (see `src/fixture_gen.rs`). Covers the slide-title/`---`-separator
    /// structure, bold formatting, bullets, and embedded images together.
    #[test]
    fn test_pptx_to_markdown_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.pptx");
        let dir = std::env::temp_dir().join(format!("pourdown-pptx-fixture-{}", std::process::id()));
        let mut sink = MediaSink::new(dir.clone());

        let md = pptx_to_markdown(path, &mut sink).expect("pptx_to_markdown should succeed");

        assert!(md.contains("# Slide One"), "slide 1 title missing:\n{md}");
        assert!(md.contains("**Bold intro**"), "bold body text missing:\n{md}");
        assert!(md.contains("---"), "slide separator missing:\n{md}");
        assert!(md.contains("# Slide Two"), "slide 2 title missing:\n{md}");
        assert!(md.contains("- First bullet"), "bullet not detected:\n{md}");
        assert!(md.contains("![](assets/image1.png)"), "image link missing:\n{md}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
