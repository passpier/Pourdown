# Markdown Import

This document explains how Pourdown's document-import feature works, where the
idea came from, and how each format is converted.

## Overview

`File → Import` converts a Word, Excel, PDF, or PowerPoint file into a new
Markdown document that opens immediately in the editor. Import is **one-way**:
source format → Markdown. Pourdown is not a round-trip converter — exporting
back to the original format will not restore the original layout exactly.

## Inspiration & credit

The idea of converting arbitrary documents into clean, LLM-friendly Markdown —
and the rationale that Markdown is more token-efficient than raw PDF/JSON for
downstream processing — was popularized by Microsoft's
[**MarkItDown**](https://github.com/microsoft/markitdown). Pourdown's import
feature was **inspired by that concept**.

To be clear about what that means in practice: **Pourdown does not use or
adapt any MarkItDown code.** It is an independent reimplementation, written in
Rust, using an entirely different set of libraries. It is not a port, and not
a fork.

Both projects are MIT-licensed, so no legal notice is required — this section
exists purely to credit the origin of the idea.

## How Pourdown differs from MarkItDown

| | MarkItDown | Pourdown |
|---|---|---|
| Language | Python | Rust |
| Approach | Often converts via an intermediate HTML representation | Converts each format directly to Markdown |
| Word | `mammoth` | `docx-rs` |
| Spreadsheet | `openpyxl` | `calamine` |
| PDF | `pdfminer` | `pdfium-render` |
| PowerPoint | `python-pptx` | Manual ZIP + XML parsing |

## Import pipeline

1. User selects **File → Import** and picks a file.
2. The frontend calls the Tauri `import_document` command (see
   `src-tauri/src/main.rs`), which dispatches by file extension and runs the
   conversion on a background thread (`tokio::task::spawn_blocking`) so the UI
   stays responsive.
3. The matching converter in `src-tauri/src/convert/{docx,xlsx,pdf,pptx}.rs`
   turns the source file into a Markdown string.
4. The returned Markdown opens as a new document in the Tiptap editor (or is
   viewable in raw Source mode).

The reverse path (`export_document`) is intentionally narrower than import: it
writes Markdown out to **HTML** (`convert::html::markdown_to_html`) or **PDF**
(`convert::pdf::markdown_to_pdf`) only. Office export (docx/xlsx/pptx) was
removed — Pourdown's core value is importing rich formats into editable
Markdown, not faithfully round-tripping Office layout/styling back out, which
Markdown can't represent anyway. HTML and PDF are the two general-purpose,
layout-controllable targets Markdown naturally maps to (web share/embed and
standardized read/print, respectively).

## Per-format conversion approach

### Word (`.docx`) — `docx-rs`

- Headings are detected from paragraph style IDs (`Heading1`–`Heading6`) with
  a fallback to the paragraph's outline level.
- Bold, italic, and strikethrough runs are mapped to `**`, `*`, and `~~`
  markers; adjacent runs with identical formatting are merged to avoid
  artifacts like `****`.
- Numbered vs. bulleted lists are resolved via the document's numbering
  definitions (abstract numbering ID → format), with nested indentation.
  Tables become GFM tables.
- External hyperlinks become `[text](url)`; internal anchor links are flattened
  to plain text.
- A table of contents is replaced with an HTML comment placeholder rather than
  being reconstructed.
- Embedded pictures (`word/media/*`) are extracted and written as sidecar
  files next to the imported document, referenced with a real `![]()` Markdown
  image link in place of the original run. Vector formats the webview can't
  render (EMF/WMF) fall back to an `*(unsupported image)*` note instead.

### Spreadsheet (`.xlsx` / `.xls` / `.ods`) — `calamine`

- Each worksheet becomes a `##` section followed by a full GFM table.
- Columns whose header text looks like a date (e.g. contains "date" or "日期")
  have their numeric values reinterpreted as Excel date serials and formatted
  as ISO dates (`YYYY-MM-DD`).
- "Continuation rows" — where a long cell pushes trailing columns onto the next
  physical row — are merged back into the previous row when the two rows'
  non-empty cells don't overlap.
- Capped at 500 data rows per sheet, with an inline note when rows are omitted.
- Embedded pictures (`xl/media/*`) are extracted as sidecar files. calamine
  doesn't expose which cell/sheet a picture belongs to, so they're listed in a
  best-effort "Embedded Images" section rather than placed inline.

### PDF — `pdfium-render`

- Text is extracted per page as positioned blocks (x/y coordinates + font
  size), not as a raw text stream.
- Blocks are grouped into visual lines by vertical proximity, then sorted
  top-to-bottom and left-to-right to reconstruct reading order.
- Heading levels are inferred from font-size ratio relative to the page's
  median (body) font size, with an ALL-CAPS short-line heuristic as a fallback
  when font sizes don't vary.
- Large vertical gaps between lines insert a blank line to preserve paragraph
  breaks. An import notice is prepended noting that layout is inferred, not
  exact.
- Tables are detected with conservative geometry clustering: visual lines are
  segmented into cells on large horizontal gaps, and a table region only
  starts where at least three consecutive lines share the same ≥2 column
  x-positions (`detect_table_regions` in `convert/pdf.rs`) — this avoids
  misreading incidental two-line alignment (e.g. a "Name: / Role:" pair) as a
  table. A wrapped cell that continues onto its own physical line (its text
  aligns under one interior column, other columns empty) is merged back into
  the row above with `<br>`. Detected regions render as GFM tables. As a
  hybrid confirming signal, ruling lines drawn as PDF path objects are used
  to refuse a continuation merge across an explicit row separator — this
  never loosens the geometry gate, so borderless tables are unaffected.
- Embedded images are extracted from each page's image objects and written as
  sidecar files, positioned in reading order alongside the surrounding text;
  exact placement is approximate for complex layouts.

### PowerPoint (`.pptx`) — manual ZIP + XML parsing

- The `.pptx` archive is read directly (it's a ZIP of XML parts); slides are
  parsed without a dedicated OOXML presentation crate.
- Each slide becomes a section, separated by `---`; the slide title placeholder
  becomes a `#` heading.
- Body paragraphs preserve bullet/indent level and basic bold/italic
  formatting.
- Image relationships are resolved from each slide's `.rels` file; the
  referenced picture is extracted from `ppt/media/*` and written as a sidecar
  file, rendered inline as a real `![]()` Markdown image link.

## Image handling

Embedded images across all four formats are written as sidecar files next to
the imported document (an `assets/` folder while the document is unsaved,
relocated to `<name>.assets/` alongside the `.md` on first save) and rendered
live in the Tiptap editor via Tauri's asset protocol. The `.md` file itself
only ever stores the relative path, so the document and its image folder stay
portable together.

Vector formats the webview can't display (EMF/WMF, common in Office exports)
are not converted — they're replaced with an `*(unsupported image)*` note
rather than a broken image link.

> Optional image captioning via an external vision-capable LLM (MarkItDown-style,
> opt-in, off by default) is planned as a follow-up but not yet implemented.

## Known limitations

- xlsx import is capped at 500 rows per sheet; embedded images can't be
  mapped to a specific sheet/cell and are listed separately.
- PDF import infers layout, not an exact reconstruction; image placement in
  complex/multi-column layouts is approximate. Table detection is
  conservative by design: a table whose wrapped cell content is itself an
  indented/bulleted list (outside the column-alignment tolerance) drops that
  row back to plain paragraphs rather than corrupting the table.
- docx, pptx, and PDF images in vector formats (EMF/WMF) can't be rendered by
  the webview and are replaced with a text note.
- pptx animations are dropped (not representable in Markdown).
