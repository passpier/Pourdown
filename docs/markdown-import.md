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

The reverse path (`export_document`) uses the corresponding `markdown_to_*`
function in the same modules to write Markdown back out to docx/xlsx/pdf/pptx.

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

### Spreadsheet (`.xlsx` / `.xls` / `.ods`) — `calamine`

- Each worksheet becomes a `##` section followed by a full GFM table.
- Columns whose header text looks like a date (e.g. contains "date" or "日期")
  have their numeric values reinterpreted as Excel date serials and formatted
  as ISO dates (`YYYY-MM-DD`).
- "Continuation rows" — where a long cell pushes trailing columns onto the next
  physical row — are merged back into the previous row when the two rows'
  non-empty cells don't overlap.
- Capped at 500 data rows per sheet, with an inline note when rows are omitted.

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

### PowerPoint (`.pptx`) — manual ZIP + XML parsing

- The `.pptx` archive is read directly (it's a ZIP of XML parts); slides are
  parsed without a dedicated OOXML presentation crate.
- Each slide becomes a section, separated by `---`; the slide title placeholder
  becomes a `#` heading.
- Body paragraphs preserve bullet/indent level and basic bold/italic
  formatting.
- Image relationships are resolved from each slide's `.rels` file and rendered
  as `[Image: filename]` placeholders (the image itself is not embedded).

## Known limitations

- xlsx import is capped at 500 rows per sheet.
- PDF import is text-only (no images or exact layout reconstruction).
- docx import skips embedded images.
- pptx import has no images or animations (image references become text
  placeholders; animations are dropped).
