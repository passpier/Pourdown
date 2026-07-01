# MarkBear

**Turn any document into clean, editable Markdown.**

[![GitHub release](https://img.shields.io/github/v/release/passpier/MarkBear?label=Download&color=0969da)](https://github.com/passpier/MarkBear/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey)](#install-an-unsigned-development-desktop-build)
[![Website](https://img.shields.io/badge/website-passpier.github.io%2FMarkBear-blue)](https://passpier.github.io/MarkBear/)

A desktop Markdown editor that imports Word files, spreadsheets, PDFs, and presentations in one click, then lets you write and edit with a live visual preview — free, offline, and open source.

**🌐 Website:** https://passpier.github.io/MarkBear/

## Screenshots

<table>
  <tr>
    <td><img src="./screenshots/home.png" alt="Home Page" width="100%"></td>
    <td><img src="./screenshots/home_without_sidebar.png" alt="Home Without Sidebar Page" width="100%"></td>
  </tr>
</table>

## Import from Other Formats

Most writing lives in Word documents, spreadsheets, or slide decks — formats that are hard to version-control, collaborate on, or publish as-is. MarkBear lets you import any of them directly into an editable Markdown document, so you can clean up, restructure, and export without manual copy-pasting.

## Why Markdown Import?

Files are converted to Markdown before processing to minimize token usage.
Community benchmarks show Markdown is ~15% more token-efficient than JSON,
and up to 96% more efficient than raw PDF when fed to LLMs

**How to import:** File → Import → choose your format. The file opens immediately as a new Markdown document.

<table>
  <tr>
    <td><img src="./screenshots/import_result.png" alt="Import result — Word document converted to Markdown" width="100%"></td>
  </tr>
</table>

### What gets converted

| Format | What's preserved | Known limitations |
|--------|-----------------|-------------------|
| **Word (.docx)** | Headings (styles + outline level), bold / italic / strikethrough, nested bullet and numbered lists, tables, hyperlinks | Images are skipped; tracked changes and comments are dropped; a TOC placeholder is inserted |
| **Spreadsheet (.xlsx / .xls / .ods)** | Each sheet becomes a section with a full GFM table; date columns are auto-detected and formatted as ISO dates | Capped at 500 rows per sheet to keep the document manageable |
| **PDF** | Headings inferred from font-size ratios; paragraph flow sorted top-to-bottom | Text-based PDFs only — scanned / image PDFs are not supported; complex multi-column layouts may reorder |
| **PowerPoint (.pptx)** | Slide titles become `#` headings; body text becomes paragraphs, one slide per section | Images and animations are not captured |

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

Download the installer for your platform from the [latest release](https://github.com/passpier/MarkBear/releases/latest), then open a terminal **in the folder containing the downloaded file** and run the commands below. The `*` glob matches any version — no edits needed when a new release ships.

#### macOS (`.dmg`)

```bash
# 1) Mount DMG  (Apple Silicon)
hdiutil attach MarkBear_*_aarch64.dmg
# On Intel Mac use: hdiutil attach MarkBear_*_x64.dmg

# 2) Copy app into Applications
cp -R "/Volumes/MarkBear/MarkBear.app" "/Applications/"

# 3) Remove quarantine flag so macOS can open this unsigned app
xattr -dr com.apple.quarantine "/Applications/MarkBear.app"

# 4) Start app
open -a "MarkBear"
```

> **Note:** download only the `.dmg` for your architecture so the glob matches exactly one file.

#### Windows (`.msi` or `.exe`)

Open PowerShell in the folder containing the installer, then:

```powershell
# Remove Mark-of-the-Web and install in one step
Get-ChildItem MarkBear_*_x64_en-US.msi | Unblock-File
msiexec /i (Get-ChildItem MarkBear_*_x64_en-US.msi).FullName
```

For `.exe` installers, SmartScreen may still require a one-time manual "More info" → "Run anyway".

## License

MIT
