---
layout: home
title: Pourdown — Markdown editor that imports Word, Excel, PDF & PowerPoint
description: Desktop Markdown editor that converts Word, Excel, PDF, and PowerPoint files into clean, editable Markdown — with live visual (WYSIWYG) editing. Built with Tauri v2, React, and Rust. Free and open source.
---

<div class="pourdown-home">

<section class="pd-hero">
  <div class="pd-hero-glow"></div>
  <div class="pd-container pd-hero-inner">
    <span class="pd-badge-pill">Free · Offline · Open source</span>
    <h1>Turn any document into<br><span class="pd-hero-accent">clean, editable Markdown</span></h1>
    <p class="pd-tagline">
      Drop in a Word doc, spreadsheet, PDF, or slide deck and get tidy
      Markdown in one click — then edit it visually. Perfect for feeding
      clean context to your LLM.
    </p>
    <div class="pd-dl-row">
      <a class="pd-dl-btn pd-dl-btn-primary" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M16.365 1.43c0 1.14-.493 2.27-1.177 3.08-.744.9-2.02 1.57-3.043 1.57-.12 0-.245-.02-.315-.03-.015-.06-.045-.226-.045-.394 0-1.14.539-2.27 1.222-3.05.723-.83 2.01-1.484 3.032-1.529.03.086.06.212.06.343zm4.243 12.4c-.03.086-.478 1.66-1.58 3.284-.949 1.396-1.933 2.79-3.48 2.816-1.524.03-2.014-.9-3.756-.9-1.744 0-2.286.87-3.728.93-1.492.06-2.63-1.51-3.585-2.9-1.95-2.86-3.442-8.08-1.44-11.607.994-1.75 2.77-2.86 4.702-2.89 1.462-.03 2.843.985 3.756.985.91 0 2.596-1.22 4.377-1.04.746.03 2.84.302 4.185 2.27-.107.067-2.5 1.46-2.47 4.35.03 3.45 3.02 4.6 3.05 4.62z"/>
        </svg>
        <span class="pd-dl-text">
          <span>Download for macOS</span>
          <span class="pd-dl-sub">Apple Silicon &amp; Intel</span>
        </span>
      </a>
      <a class="pd-dl-btn" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M3 5.5L10.4 4.5V11.5H3V5.5ZM11.3 4.4L21 3V11.4H11.3V4.4ZM3 12.5H10.4V19.6L3 18.5V12.5ZM11.3 12.5H21V21L11.4 19.6L11.3 12.5Z"/>
        </svg>
        <span class="pd-dl-text">
          <span>Download for Windows</span>
          <span class="pd-dl-sub">.msi installer</span>
        </span>
      </a>
    </div>
    <p class="pd-trust">Runs entirely on your machine. No account, no upload, no runtime to install.</p>
    <div class="pd-demo-wrap">
      <div class="pd-demo-frame">
        <div class="pd-demo-bar">
          <span class="pd-demo-dot red"></span>
          <span class="pd-demo-dot yellow"></span>
          <span class="pd-demo-dot green"></span>
          <span class="pd-demo-title">report.pdf → report.md</span>
        </div>
        <video class="pd-demo-video" controls preload="none" playsinline poster="/demo-poster.webp" width="1108" height="720">
          <source src="/demo.mp4" type="video/mp4">
          <a href="/Pourdown/demo.mp4">Watch the import demo</a>
        </video>
      </div>
      <p class="pd-demo-caption">See it in action — importing a PDF and getting editable Markdown back.</p>
    </div>
  </div>
</section>

<section id="formats" class="pd-section pd-section-alt">
  <div class="pd-container">
    <div class="pd-section-head">
      <h2>Import from any format</h2>
      <p class="pd-section-sub">
        Pourdown converts your files to Markdown and keeps your images —
        most converters just throw them away. Headings, lists, tables and
        links come along too.
      </p>
    </div>
    <div class="pd-format-grid">
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-docx">.docx</div>
        <h3>Word</h3>
        <p>Headings, bold &amp; italic, nested lists, tables, links and embedded images — all preserved.</p>
      </div>
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-xlsx">.xlsx</div>
        <h3>Excel</h3>
        <p>Each sheet becomes a clean table, dates auto-formatted, embedded images pulled out.</p>
      </div>
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-pdf">.pdf</div>
        <h3>PDF</h3>
        <p>Headings and reading order inferred, tables rebuilt, images extracted. (Text-based PDFs.)</p>
      </div>
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-pptx">.pptx</div>
        <h3>PowerPoint</h3>
        <p>Slide titles become headings, body text becomes paragraphs, images extracted.</p>
      </div>
    </div>
    <p class="pd-format-foot">Exactly what's kept and today's limitations are in the <a href="/guide/importing">Importing guide →</a></p>
  </div>
</section>

<section class="pd-section">
  <div class="pd-container">
    <div class="pd-llm-band">
      <div class="pd-llm-inner">
        <div class="pd-llm-kicker">Made for prompting</div>
        <p class="pd-llm-title">Feed cleaner context to your LLM — for a fraction of the tokens.</p>
        <p class="pd-llm-sub">Markdown is up to 96% more token-efficient than raw PDF when fed to a language model (<a href="/guide/importing">community benchmarks</a>). Convert once, prompt for less.</p>
      </div>
    </div>
  </div>
</section>

<section id="features" class="pd-section pd-section-alt">
  <div class="pd-container">
    <div class="pd-section-head">
      <h2>Everything you need to write</h2>
      <p class="pd-section-sub">A focused editor that gets out of your way.</p>
    </div>
    <div class="pd-feature-grid">
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5Z"/></svg>
        </div>
        <h3>Visual editing</h3>
        <p>Write the way it looks — no raw Markdown symbols to wrestle with.</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/></svg>
        </div>
        <h3>Keeps your images</h3>
        <p>Embedded pictures are extracted and shown inline, not dropped on the floor.</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        </div>
        <h3>Offline &amp; private</h3>
        <p>Every conversion happens on your device. Your documents never leave it.</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><circle cx="7.5" cy="10.5" r="1"/><circle cx="12" cy="7.5" r="1"/><circle cx="16.5" cy="10.5" r="1"/><circle cx="10" cy="15" r="1"/><path d="M12 22a5 5 0 0 1-1-9.9 2 2 0 0 0 1.5-3.1A9.98 9.98 0 0 1 12 2"/></svg>
        </div>
        <h3>Themes to match your taste</h3>
        <p>Seven built-in editor themes — GitHub Light/Dark, Dracula, Nord, Solarized and more.</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
        </div>
        <h3>Rich Markdown syntax</h3>
        <p>Headings, tables, task lists, code blocks, blockquotes and more — full GitHub-flavored Markdown.</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z"/></svg>
        </div>
        <h3>English &amp; 中文</h3>
        <p>A fully bilingual interface — English and Traditional Chinese, switchable anytime.</p>
      </div>
    </div>
  </div>
</section>

<section class="pd-section">
  <div class="pd-container pd-native">
    <h2>Native, light, and instant</h2>
    <p>
      Built on Tauri v2 instead of Electron — it uses your system's built-in
      webview rather than bundling a whole browser, so the download stays
      small, it opens fast, and there's nothing extra to install before your
      first conversion.
    </p>
  </div>
</section>

<section class="pd-section pd-section-alt">
  <div class="pd-container pd-cta-band">
    <img src="/logo-64.webp" alt="Pourdown" width="56" height="56">
    <h2>Get Pourdown</h2>
    <p class="pd-section-sub">Convert your first document to clean Markdown in the next minute.</p>
    <div class="pd-dl-row">
      <a class="pd-dl-btn pd-dl-btn-primary" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M16.365 1.43c0 1.14-.493 2.27-1.177 3.08-.744.9-2.02 1.57-3.043 1.57-.12 0-.245-.02-.315-.03-.015-.06-.045-.226-.045-.394 0-1.14.539-2.27 1.222-3.05.723-.83 2.01-1.484 3.032-1.529.03.086.06.212.06.343zm4.243 12.4c-.03.086-.478 1.66-1.58 3.284-.949 1.396-1.933 2.79-3.48 2.816-1.524.03-2.014-.9-3.756-.9-1.744 0-2.286.87-3.728.93-1.492.06-2.63-1.51-3.585-2.9-1.95-2.86-3.442-8.08-1.44-11.607.994-1.75 2.77-2.86 4.702-2.89 1.462-.03 2.843.985 3.756.985.91 0 2.596-1.22 4.377-1.04.746.03 2.84.302 4.185 2.27-.107.067-2.5 1.46-2.47 4.35.03 3.45 3.02 4.6 3.05 4.62z"/>
        </svg>
        <span>Download for macOS</span>
      </a>
      <a class="pd-dl-btn" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M3 5.5L10.4 4.5V11.5H3V5.5ZM11.3 4.4L21 3V11.4H11.3V4.4ZM3 12.5H10.4V19.6L3 18.5V12.5ZM11.3 12.5H21V21L11.4 19.6L11.3 12.5Z"/>
        </svg>
        <span>Download for Windows</span>
      </a>
    </div>
  </div>
</section>

</div>
