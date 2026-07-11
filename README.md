<p align="center">
  <img src="./app-icon.png" alt="Pourdown icon" width="128" height="128">
</p>

<h1 align="center">Pourdown</h1>

<p align="center"><strong>Turn any document into clean, editable Markdown.</strong></p>

<p align="center">
  <a href="https://github.com/passpier/Pourdown/releases/latest"><img src="https://img.shields.io/github/v/release/passpier/Pourdown?label=Download&color=0969da" alt="GitHub release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-green.svg" alt="License: MIT"></a>
  <a href="#install-an-unsigned-development-desktop-build"><img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey" alt="Platform"></a>
  <a href="https://passpier.github.io/Pourdown/"><img src="https://img.shields.io/badge/website-passpier.github.io%2FPourdown-blue" alt="Website"></a>
</p>

<p align="center">A desktop Markdown editor that imports Word files, spreadsheets, PDFs, and presentations in one click, then lets you write and edit with a live visual preview — free, offline, and open source.</p>

<p align="center"><a href="https://passpier.github.io/Pourdown/"><strong>🌐 Website</strong></a></p>

## Demo

<p align="center">
  <video src="https://github.com/user-attachments/assets/cd323eec-f064-475b-a209-9153d4d576ce" controls width="720"></video>
</p>

<p align="center"><em>Importing a PDF and getting editable Markdown back in one click.</em></p>

## Screenshots

<table>
  <tr>
    <td><img src="./screenshots/home_with_no_file_sidebar.png" alt="Home Page" width="100%"></td>
    <td><img src="./screenshots/home_with_outline_sidebar.png" alt="Home Without Sidebar Page" width="100%"></td>
  </tr>
</table>

## Import from Other Formats

Most writing lives in Word documents, spreadsheets, or slide decks — formats that are hard to version-control, collaborate on, or publish as-is. Pourdown lets you import any of them directly into an editable Markdown document, so you can clean up, restructure, and export without manual copy-pasting.

## Why Markdown Import?

Files are converted to Markdown before processing to minimize token usage.
Community benchmarks show Markdown is ~15% more token-efficient than JSON,
and up to 96% more efficient than raw PDF when fed to LLMs

This approach was inspired by Microsoft's [MarkItDown](https://github.com/microsoft/markitdown);
see [`markdown-import.md`](markdown-import.md) for how Pourdown's
Rust implementation works and how it differs.

### What gets converted

| Format | What's preserved | Known limitations |
|--------|-----------------|-------------------|
| **Word (.docx)** | Headings (styles + outline level), bold / italic / strikethrough, nested bullet and numbered lists, tables, hyperlinks, embedded images | Vector images (EMF/WMF) can't be displayed; tracked changes and comments are dropped; a TOC placeholder is inserted |
| **Spreadsheet (.xlsx / .xls / .ods)** | Each sheet becomes a section with a full GFM table; date columns are auto-detected and formatted as ISO dates; embedded images are extracted | Capped at 500 rows per sheet; images can't be mapped to a specific cell/sheet |
| **PDF** | Headings inferred from font-size ratios; paragraph flow sorted top-to-bottom; tables are detected and rendered as GFM tables; Table of Contents entries render as a bulleted list; embedded images are extracted | Text-based PDFs only — scanned / image PDFs are not supported; complex multi-column layouts may reorder; tables with deeply nested/bulleted wrapped cells may fall back to plain text |
| **PowerPoint (.pptx)** | Slide titles become `#` headings; body text becomes paragraphs, one slide per section; embedded images are extracted | Animations are not captured; vector images (EMF/WMF) can't be displayed |

Extracted images are saved as sidecar files next to the imported document (an `assets/` folder, relocated to `<name>.assets/` alongside the `.md` once saved) and render live in the editor.

> Import converts content to Markdown. It is not a round-trip format converter — exporting back to the original format will not restore the original layout exactly.

## Features

- **Document Import** — Convert Word, Excel, PDF, and PowerPoint files to Markdown in one click
- **Visual Markdown Editing** — Write and edit Markdown the way you see it, without dealing with raw symbols
- **Source Mode** — Toggle to raw Markdown text at any time
- **File Management** — Open, save, and manage your Markdown files using native system dialogs
- **Rich Text Formatting** — Bold, italic, lists, code blocks, blockquotes, and more
- **Auto-save** — Your work is saved automatically at regular intervals
- **Find & Replace** — In-document search with replace support; cross-file search in the sidebar
- **Multiple Themes** — Choose from seven built-in UI themes
- **i18n** — English and Traditional Chinese interface

## Tech Stack

- **Frontend**: React 18 + TypeScript
- **Editor**: Tiptap v2 with extensions
- **Desktop**: Tauri v2
- **State Management**: Zustand
- **Styling**: Tailwind CSS
- **UI Components**: Custom components with shadcn/ui patterns
- **Build Tool**: Vite

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (v20 or later)
- [pnpm](https://pnpm.io/) (or npm/yarn)
- [Rust](https://www.rust-lang.org/) (for Tauri desktop builds)

### Development

Start the development server with hot reload:

```bash
pnpm dev
```

For Tauri-specific development:

```bash
pnpm tauri dev
```

### Building

Create an optimized production build:

```bash
pnpm build
```

Create the desktop application:

```bash
pnpm tauri build
```

### Install an unsigned development desktop build

Unsigned builds are intended for local testing only. For normal distribution, use code signing/notarization.

Download the installer for your platform from the [latest release](https://github.com/passpier/Pourdown/releases/latest), then open a terminal **in the folder containing the downloaded file** and run the commands below. The `*` glob matches any version — no edits needed when a new release ships.

#### macOS (`.dmg`)

```bash
# 1) Mount DMG  (Apple Silicon)
hdiutil attach Pourdown_*_aarch64.dmg
# On Intel Mac use: hdiutil attach Pourdown_*_x64.dmg

# 2) Copy app into Applications
cp -R "/Volumes/Pourdown/Pourdown.app" "/Applications/"

# 3) Remove quarantine flag so macOS can open this unsigned app
xattr -dr com.apple.quarantine "/Applications/Pourdown.app"

# 4) Start app
open -a "Pourdown"
```

> **Note:** download only the `.dmg` for your architecture so the glob matches exactly one file.

#### Windows (`.msi` or `.exe`)

Open PowerShell in the folder containing the installer, then:

```powershell
# Remove Mark-of-the-Web and install in one step
Get-ChildItem Pourdown_*_x64_en-US.msi | Unblock-File
msiexec /i (Get-ChildItem Pourdown_*_x64_en-US.msi).FullName
```

For `.exe` installers, SmartScreen may still require a one-time manual "More info" → "Run anyway".

## Contributing

Contributions are welcome! See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup
steps, the test-first workflow for conversion changes, and the PR process.
The [docs site](https://passpier.github.io/Pourdown/) has a full user guide
(English / 繁體中文) if you want to understand a feature in more depth than
this README covers.

## Acknowledgements

The Markdown Import feature was inspired by Microsoft's
[MarkItDown](https://github.com/microsoft/markitdown) — both projects are
MIT-licensed. Pourdown is an independent reimplementation in Rust (not a fork
or port); see [`markdown-import.md`](markdown-import.md) for details
on how the two differ.

## License

MIT
