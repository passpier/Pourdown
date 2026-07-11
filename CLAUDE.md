# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Pourdown is a Tauri v2 desktop Markdown editor (React 18 + TypeScript frontend, Rust backend) that imports Word/Excel/PowerPoint/PDF files into Markdown and edits them with a Tiptap WYSIWYG editor plus a raw Source mode.

## Structure

- `src/` — React frontend (Vite, TypeScript, Tailwind CSS, Tiptap editor, Zustand state, i18next)
  - `components/{Sidebar,Toolbar,Search,CodeBlockRenderer,Editor}`, `stores/`, `extensions/`, `theme/`, `hooks/`, `lib/`, `i18n/locales/`
- `src-tauri/` — Rust backend, single crate named `Pourdown` (no Cargo workspace, no `lib.rs` — everything is a module of the `Pourdown` binary)
  - `src/convert/{docx,pdf,pptx,xlsx,html}.rs` — per-format conversion to/from Markdown, each with an inline `#[cfg(test)]` test module (unit tests for pure helpers + fixture-backed end-to-end tests)
  - `src/fixture_gen.rs` — regenerates the binary fixtures under `tests/fixtures/sample.{docx,xlsx,pptx,pdf}` used by those end-to-end tests (test-only, `#[ignore]`d by default; see its doc comment)
  - `tauri.conf.json`, `capabilities/`, `permissions/`
- No frontend test runner is set up — only the Rust conversion logic has automated tests.

## Commands

- `pnpm dev` — Vite dev server (frontend only)
- `pnpm tauri dev` — full Tauri dev app
- `pnpm build` — `tsc && vite build`
- `pnpm tauri build` — production desktop build
- `pnpm lint` — `tsc --noEmit && eslint .` (ESLint flat config at `eslint.config.js`)
- `cd src-tauri && cargo clippy --all-targets` — Rust lint
- `cd src-tauri && cargo test` — runs the conversion test suite (inline `#[cfg(test)]` modules per converter, fixture-backed end-to-end tests under `tests/fixtures/`). This is the **primary** way to verify a change to `src-tauri/src/convert/*.rs` — prefer it over manually running the app; see `/verify-conversion` skill for the test-first workflow and when manual app verification is still the right fallback.
- Package manager is **pnpm** (not npm/yarn) — `pnpm-lock.yaml` is present.

## Rust crate API gotchas (verified against installed versions)

- **pulldown-cmark 0.13** — GFM table header row has NO `TableRow` wrapper; cells sit directly in `TableHead` (`Start(TableHead) → Start(TableCell)… → End(TableHead)`). Data rows DO have `TableRow` wrappers. Capture cells in a `current_row` buffer during `TableHead` and save to `header_row` on `End(TableHead)`.
- **calamine 0.33** — `DataType` is a trait, not the cell enum. The concrete enum is `Data` (`use calamine::{Data, Reader}`). No `Duration` variant; use `DurationIso(String)`. `worksheet_range` returns `Result<Range<Data>, _>`.
- **docx-rs 0.4.x** — `read_docx(buf: &[u8]) -> Result<Docx, ReaderError>` returns a `Result` directly (no `.parse()`). Body children live in `docx.document.children: Vec<DocumentChild>`, with variants `DocumentChild::Paragraph(Box<Paragraph>)` and `DocumentChild::Table(Box<Table>)`. `Bold.val` is private — use `.is_some()` as a proxy for whether bold is enabled.
- **markdown2pdf 0.2.x** — API is `markdown2pdf::parse_into_file(content: String, path: &str, ConfigSource::Default, None)`, not `markdown_to_pdf`. Import `markdown2pdf::config::ConfigSource`.
- **PDF import uses `pdfium-render 0.9`** (not `pdf-extract`) and requires a PDFium library to be available at runtime.
- **Tauri v2 menus** — `event.id()` returns `&MenuId`; use `.0` for string ops (`event.id().0.starts_with(...)`). `menu.get(&id)` returns `Option<MenuItemKind<R>>`. To enable/disable items, match on the `MenuItemKind` variant and call `.set_enabled()` on each arm.

## Image-preserving import (docx/pptx/pdf/xlsx)

All four importers extract embedded images as sidecar files (`convert/media.rs`
`MediaSink`) and emit real `![]()` links instead of dropping/placeholder-ing
them. Images are written to `imports/{id}/assets/` (via `import_document` in
`main.rs`) and relocated to `<name>.assets/` next to the `.md` on first save
(`relocate_media` command, wired from `documentStore.ts` `saveDocument`).
Rendering in the Tiptap editor goes through Tauri's asset protocol
(`tauri.conf.json` `app.security.assetProtocol`) — see `CustomImage.renderHTML`
in `Editor.tsx`, which resolves the document's `assetDir` via `convertFileSrc`.

## Known conversion limitations (document, don't "fix" without discussion)

- xlsx import is capped at 500 rows per sheet; embedded images can't be
  mapped to a specific sheet/cell (best-effort "Embedded Images" section).
- PDF import infers layout, not an exact reconstruction; image placement is
  approximate for complex layouts. Tables are detected via conservative
  geometry clustering (`detect_table_regions` in `convert/pdf.rs`, requires
  ≥2 aligned columns across ≥3 consecutive rows) and rendered as GFM tables,
  with wrapped cells merged back via `<br>`; a cell whose wrapped content is
  itself a bulleted/indented list falls outside the alignment tolerance and
  drops that row back to prose instead of corrupting the table — a
  deliberate conservative trade-off, not a bug. Dot-leader lines (Table of
  Contents entries, e.g. `Introduction .... 5`) are explicitly excluded from
  table detection (`row_has_dot_leader` in `convert/pdf.rs`) and instead
  rendered as a flat bulleted list with the leader collapsed to `…`, since
  they otherwise satisfy the column-alignment gates but aren't tabular data.
- Standard two-column journal layouts (e.g. IEEE Access) are detected
  (`detect_gutter` in `convert/pdf.rs`, requires ≥4 lines with independent
  left/right content — same "repeated structural evidence" gate as table
  detection, so an incidentally-spaced title line doesn't false-positive)
  and read column-by-column via `segment_page`/`render_region`, instead of
  being misread as false 2-cell tables. Within `segment_page`, a line is
  full-width (not split into the two-column band) if some run on it spans
  the gutter, *or* its left/right content sit close enough together
  (`GUTTER_LINE_GAP_FACTOR`) to be one continuous run pdfium happened to
  split near the gutter — e.g. a heading label immediately followed by its
  text, like "ABSTRACT " + the abstract's first line — rather than genuine
  independent column content; a real column gutter is a much wider empty
  margin band than an ordinary run-boundary gap. Body lines are reflowed into
  paragraphs with hyphen-aware de-hyphenation (`append_wrapped`) — only
  when the PDF's text stream has a literal hyphen glyph at the wrap point;
  some PDFs wrap without one, leaving a raw space (e.g. "bet ter"), which
  isn't fixable from the extracted text alone. 3+ column layouts, irregular
  column widths, and rotated text aren't handled (fall back to
  single-column reading order). Repeated running headers/footers (page
  numbers, author/title strips) are stripped: `detect_running_headers_footers`
  in `convert/pdf.rs` scans each page's top/bottom margin band
  (`HF_BAND_FRACTION`), normalizes digit runs to `#` so incrementing page
  numbers compare equal (`normalize_hf`), and flags any band line recurring
  on ≥`HF_MIN_PAGES` (3) distinct pages — the same "repeated structural
  evidence" gate used for table/gutter detection — before any page is
  rendered, so a page number fused into body text (e.g. "…components. 18913")
  is removed too, not just standalone header/footer lines. A header/footer
  recurring on fewer than 3 pages, or one whose text is fused with unique
  per-page content (e.g. a page-1 footer merged into a copyright notice), is
  conservatively left in place.
- Vector image formats (EMF/WMF, common in Office exports) can't be rendered
  by the webview — replaced with an `*(unsupported image)*` note.
- pptx animations are dropped (not representable in Markdown).
- Optional LLM-vision image captioning (opt-in, off by default) is planned
  but not yet implemented — see `markdown-import.md`.
