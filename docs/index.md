---
layout: home
title: Pourdown — Markdown editor that imports Word, Excel, PDF & PowerPoint
description: Desktop Markdown editor that converts Word, Excel, PDF, and PowerPoint files into clean, editable Markdown — with live visual (WYSIWYG) editing. Built with Tauri v2, React, and Rust. Free and open source.
---

<div class="pourdown-home">

<section class="pd-hero">
  <div class="pd-container">
    <h1>Turn any document into<br>clean, editable Markdown</h1>
    <p class="pd-tagline">
      Import Word files, spreadsheets, PDFs, and presentations in one click,
      then write and edit with a live visual preview. Free, offline, open source.
    </p>
    <div class="pd-cta-row">
      <a class="pd-btn pd-btn-primary" href="https://github.com/passpier/Pourdown/releases/latest">
        ⬇ Download
      </a>
      <a class="pd-btn pd-btn-secondary" href="/guide/getting-started">
        Get Started
      </a>
      <a class="pd-btn pd-btn-secondary" href="https://github.com/passpier/Pourdown">
        View on GitHub
      </a>
    </div>
  </div>
</section>

<section class="pd-screenshot">
  <div class="pd-container">
    <img
      src="/home-860.webp"
      srcset="/home-860.webp 860w, /home-1720.webp 1720w"
      sizes="(max-width: 900px) 100vw, 860px"
      alt="Pourdown editor showing a Markdown document with file sidebar and toolbar"
      width="860"
      height="574"
      loading="lazy"
      decoding="async"
    />
  </div>
</section>

<section class="pd-section">
  <div class="pd-container">
    <h2 class="pd-section-title">Import from any format</h2>
    <p class="pd-section-sub">
      Pourdown converts Word, Excel, PDF, and PowerPoint files to Markdown
      while <strong>keeping your images</strong> — something most Markdown
      converters simply throw away. Structure comes along too: headings,
      lists, tables, and links, not just plain text. (As a bonus, Markdown is
      also up to 96% more token-efficient than raw PDF for feeding into LLMs —
      <a href="/guide/importing">community benchmarks</a>.)
    </p>

| Format | What you get |
|---|---|
| <span class="pd-badge">Word .docx</span> | Headings, bold/italic/strikethrough, nested lists, tables, hyperlinks, and embedded images — preserved |
| <span class="pd-badge">Excel .xlsx / .ods</span> | Each sheet as a clean table; dates auto-formatted; embedded images extracted |
| <span class="pd-badge">PDF</span> | Headings and reading order inferred automatically; tables detected and rebuilt; embedded images extracted |
| <span class="pd-badge">PowerPoint .pptx</span> | Slide titles become headings, body text becomes paragraphs, embedded images extracted |

<p class="pd-section-sub" style="margin-top: 20px; margin-bottom: 0;">
  See exactly what's preserved and today's known limitations in the
  <a href="/guide/importing">Importing Documents guide</a>.
</p>

  </div>
</section>

<section class="pd-features">
  <div class="pd-container">
    <h2 class="pd-section-title">Everything you need to write in Markdown</h2>
    <div class="pd-grid">
      <div class="pd-card">
        <div class="pd-card-icon">✏️</div>
        <h3>Visual Editing</h3>
        <p>Write without raw Markdown symbols using the WYSIWYG editor powered by Tiptap.</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">💻</div>
        <h3>Source Mode</h3>
        <p>Toggle to raw Markdown text at any time — full control when you need it.</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🔍</div>
        <h3>Find &amp; Replace</h3>
        <p>In-document search with replace and cross-file search in the sidebar.</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">💾</div>
        <h3>Auto-save</h3>
        <p>Your work is saved automatically at regular intervals — no lost edits.</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🎨</div>
        <h3>Seven Themes</h3>
        <p>GitHub Light/Dark, Dracula, Nord, and Solarized — pick your look.</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🌐</div>
        <h3>English &amp; 中文</h3>
        <p>English and Traditional Chinese interface out of the box.</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🔒</div>
        <h3>Offline &amp; Private</h3>
        <p>Every conversion runs on your machine — your documents never get uploaded anywhere.</p>
      </div>
    </div>
  </div>
</section>

<section class="pd-tech">
  <div class="pd-container">
    <h2 class="pd-section-title">Native &amp; lightweight</h2>
    <p class="pd-section-sub">
      Pourdown is built on <strong>Tauri v2</strong>, not Electron — it uses
      your OS's built-in webview instead of bundling an entire browser, so the
      download and memory footprint stay small. Import is native Rust, not a
      Python script, so there's no separate runtime to install. Free and open
      source under the MIT license.
    </p>
  </div>
</section>

</div>
