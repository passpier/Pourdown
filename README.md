# MarkBear

A beautiful desktop Markdown editor with a clean, visual writing experience. Write and edit Markdown documents naturally.

## Screenshots

<table>
  <tr>
    <td><img src="./screenshots/home.png" alt="Home Page" width="100%"></td>
    <td><img src="./screenshots/home_without_sidebar.png" alt="Home Without Sidebar Page" width="100%"></td>
  </tr>
</table>

## Features

- **Visual Markdown Editing** — Write and edit Markdown the way you see it, without dealing with raw symbols
- **File Management** — Open, save, and manage your Markdown files using native system dialogs
- **Rich Text Formatting** — Bold, italic, lists, code blocks, blockquotes, and more — all at your fingertips
- **Auto-save** — Your work is saved automatically at regular intervals, so you never lose progress
- **Multiple Themes** — Choose from seven built-in UI themes

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

#### macOS (`.dmg`)

```bash
# 1) Mount DMG
hdiutil attach "MarkBear_0.3.5_aarch64.dmg"

# 2) Copy app into Applications (adjust volume path if needed)
cp -R "/Volumes/MarkBear/MarkBear.app" "/Applications/"

# 3) Remove quarantine flag so macOS can open this unsigned app
xattr -dr com.apple.quarantine "/Applications/MarkBear.app"

# 4) Start app
open -a "MarkBear"
```

If your DMG file is `MarkBear_0.3.5_x64.dmg`, use that filename in step 1.

#### Windows (`.msi` or `.exe`)

Open PowerShell in the folder containing the installer, then:

```powershell
# Optional: remove Mark-of-the-Web on downloaded file first
Unblock-File .\MarkBear_0.3.5_x64_en-US.msi

# Install MSI from CLI
msiexec /i .\MarkBear_0.3.5_x64_en-US.msi
```

For `.exe` installers, SmartScreen may still require a one-time manual "More info" -> "Run anyway".

## License

MIT
