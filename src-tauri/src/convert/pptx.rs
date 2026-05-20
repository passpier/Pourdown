use std::collections::HashMap;
use super::ConversionError;

/// Convert a PPTX file to Markdown.
/// Each slide becomes a section separated by `---`.
pub fn pptx_to_markdown(path: &str) -> Result<String, ConversionError> {
    use std::io::Read;

    let file = std::fs::File::open(path)
        .map_err(|e| ConversionError(format!("Failed to open PPTX: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ConversionError(format!("Failed to read PPTX archive: {}", e)))?;

    // Collect rels and slide XML in a single pass
    let mut rels_map: HashMap<usize, String> = HashMap::new();
    let mut slides_raw: Vec<(usize, String)> = Vec::new();

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
        }
    }

    slides_raw.sort_by_key(|(n, _)| *n);

    let mut parts: Vec<String> = Vec::new();
    for (num, xml) in &slides_raw {
        let rels = rels_map
            .get(num)
            .map(|r| parse_slide_rels(r))
            .unwrap_or_default();
        let text = extract_slide_content(xml, &rels);
        if !text.is_empty() {
            parts.push(text);
        }
    }

    Ok(parts.join("\n\n---\n\n"))
}

/// Parse a slide rels XML and return a map of rId → image filename.
/// Only image relationships are included.
fn parse_slide_rels(rels_xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for chunk in rels_xml.split("<Relationship ") {
        if !chunk.contains("/image") {
            continue;
        }
        if let (Some(id), Some(target)) = (get_xml_attr(chunk, "Id"), get_xml_attr(chunk, "Target")) {
            let filename = target.rsplit('/').next().unwrap_or(&target).to_string();
            map.insert(id, filename);
        }
    }
    map
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

/// Extract slide content: title (as `# heading`), image placeholders, and body paragraphs.
fn extract_slide_content(xml: &str, rels: &HashMap<String, String>) -> String {
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

    // Collect content-image placeholders from r:embed references (deduplicated)
    let mut image_placeholders: Vec<String> = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = xml[search_from..].find("r:embed=\"") {
        let abs = search_from + pos + 9;
        if let Some(end) = xml[abs..].find('"') {
            let rid = &xml[abs..abs + end];
            if let Some(fname) = rels.get(rid) {
                let placeholder = format!("[Image: {}]", fname);
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
fn extract_run_text_formatted(para: &str) -> String {
    let mut result = String::new();
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
                let text = xml_decode(&after[..t_end]);
                if !text.is_empty() {
                    let formatted = match (is_bold, is_italic) {
                        (true, true) => format!("***{}***", text),
                        (true, false) => format!("**{}**", text),
                        (false, true) => format!("_{}_", text),
                        (false, false) => text,
                    };
                    result.push_str(&formatted);
                }
            }
        }
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

/// Convert Markdown to a PPTX file.
/// `# Heading` boundaries define slide splits.
/// Each heading starts a new slide; remaining content goes in the slide body.
pub fn markdown_to_pptx(markdown: &str, path: &str) -> Result<(), ConversionError> {
    // Parse slides from markdown: split on H1 headings
    let mut slides: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_title = String::new();
    let mut current_body: Vec<String> = Vec::new();

    for line in markdown.lines() {
        if line.starts_with("# ") && !line.starts_with("## ") {
            // New slide
            if !current_title.is_empty() || !current_body.is_empty() {
                slides.push((current_title.clone(), current_body.clone()));
            }
            current_title = line.trim_start_matches("# ").to_string();
            current_body.clear();
        } else if !line.trim().is_empty() {
            current_body.push(line.to_string());
        }
    }
    // Last slide
    if !current_title.is_empty() || !current_body.is_empty() {
        slides.push((current_title, current_body));
    }

    if slides.is_empty() {
        // Create a single slide with the entire content as body
        slides.push(("Presentation".to_string(),
                      markdown.lines().map(|l| l.to_string()).collect()));
    }

    build_pptx(slides, path)
}

/// Build a minimal PPTX file from slide data.
/// PPTX is a ZIP file with specific XML structure.
fn build_pptx(slides: Vec<(String, Vec<String>)>, path: &str) -> Result<(), ConversionError> {
    use std::io::Write;

    let file = std::fs::File::create(path)
        .map_err(|e| ConversionError(format!("Failed to create PPTX file: {}", e)))?;

    let mut zip = zip::ZipWriter::new(file);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // [Content_Types].xml
    zip.start_file("[Content_Types].xml", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    let mut content_types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
"#,
    );
    for i in 0..slides.len() {
        content_types.push_str(&format!(
            r#"  <Override PartName="/ppt/slides/slide{}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
"#,
            i + 1
        ));
    }
    content_types.push_str("</Types>");
    zip.write_all(content_types.as_bytes())
        .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    // _rels/.rels
    zip.start_file("_rels/.rels", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#,
    )
    .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    // ppt/_rels/presentation.xml.rels
    zip.start_file("ppt/_rels/presentation.xml.rels", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    let mut pres_rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
"#,
    );
    for i in 0..slides.len() {
        pres_rels.push_str(&format!(
            r#"  <Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{}.xml"/>
"#,
            i + 2,
            i + 1
        ));
    }
    pres_rels.push_str("</Relationships>");
    zip.write_all(pres_rels.as_bytes())
        .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    // ppt/presentation.xml
    zip.start_file("ppt/presentation.xml", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    let mut slide_list = String::new();
    for i in 0..slides.len() {
        slide_list.push_str(&format!(
            r#"    <p:sldId id="{}" r:id="rId{}"/>
"#,
            256 + i,
            i + 2
        ));
    }
    let pres_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:sldMasterIdLst>
    <p:sldMasterId id="2147483648" r:id="rId1"/>
  </p:sldMasterIdLst>
  <p:sldIdLst>
{}  </p:sldIdLst>
  <p:sldSz cx="9144000" cy="6858000"/>
  <p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#,
        slide_list
    );
    zip.write_all(pres_xml.as_bytes())
        .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    // Minimal slide master
    zip.start_file("ppt/slideMasters/slideMaster1.xml", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
  xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
  <p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles>
  <p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
</p:sldMaster>"#,
    )
    .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#,
    )
    .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    // Minimal slide layout
    zip.start_file("ppt/slideLayouts/slideLayout1.xml", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
  xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" type="blank">
  <p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
</p:sldLayout>"#,
    )
    .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", options)
        .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#,
    )
    .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

    // Individual slides
    for (i, (title, body_lines)) in slides.iter().enumerate() {
        let slide_path = format!("ppt/slides/slide{}.xml", i + 1);
        let rels_path = format!("ppt/slides/_rels/slide{}.xml.rels", i + 1);

        // Slide rels
        zip.start_file(&rels_path, options)
            .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#,
        )
        .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;

        // Slide XML
        let title_escaped = xml_escape(title);
        let body_text = body_lines.join("\n");
        let body_escaped = xml_escape(&body_text);

        let slide_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
  xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr/>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="457200" y="274638"/><a:ext cx="8229600" cy="1143000"/></a:xfrm></p:spPr>
        <p:txBody><a:bodyPr/><a:lstStyle/>
          <a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="3" name="Body"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="457200" y="1600200"/><a:ext cx="8229600" cy="4525963"/></a:xfrm></p:spPr>
        <p:txBody><a:bodyPr/><a:lstStyle/>
          <a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#,
            title_escaped, body_escaped
        );

        zip.start_file(&slide_path, options)
            .map_err(|e| ConversionError(format!("ZIP error: {}", e)))?;
        zip.write_all(slide_xml.as_bytes())
            .map_err(|e| ConversionError(format!("ZIP write error: {}", e)))?;
    }

    zip.finish()
        .map_err(|e| ConversionError(format!("Failed to finalize PPTX: {}", e)))?;

    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
