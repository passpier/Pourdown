---
layout: home
title: Pourdown — 匯入 Word、Excel、PDF 與 PowerPoint 的 Markdown 編輯器
description: 將 Word、Excel、PDF、PowerPoint 檔案轉換成乾淨、可編輯 Markdown 的桌面編輯器，並提供即時視覺化（WYSIWYG）編輯。以 Tauri v2、React 與 Rust 打造，免費、開源。
---

<div class="pourdown-home">

<section class="pd-hero">
  <div class="pd-hero-glow"></div>
  <div class="pd-container pd-hero-inner">
    <span class="pd-badge-pill">免費 · 離線 · 開源</span>
    <h1>把任何文件變成<br><span class="pd-hero-accent">乾淨、可編輯的 Markdown</span></h1>
    <p class="pd-tagline">
      拖入 Word、試算表、PDF 或簡報，一鍵得到整齊的 Markdown，接著用視覺化方式編輯。
      餵給 LLM 前的最佳前處理。
    </p>

    <div class="pd-dl-row">
      <a class="pd-dl-btn pd-dl-btn-primary" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M16.365 1.43c0 1.14-.493 2.27-1.177 3.08-.744.9-2.02 1.57-3.043 1.57-.12 0-.245-.02-.315-.03-.015-.06-.045-.226-.045-.394 0-1.14.539-2.27 1.222-3.05.723-.83 2.01-1.484 3.032-1.529.03.086.06.212.06.343zm4.243 12.4c-.03.086-.478 1.66-1.58 3.284-.949 1.396-1.933 2.79-3.48 2.816-1.524.03-2.014-.9-3.756-.9-1.744 0-2.286.87-3.728.93-1.492.06-2.63-1.51-3.585-2.9-1.95-2.86-3.442-8.08-1.44-11.607.994-1.75 2.77-2.86 4.702-2.89 1.462-.03 2.843.985 3.756.985.91 0 2.596-1.22 4.377-1.04.746.03 2.84.302 4.185 2.27-.107.067-2.5 1.46-2.47 4.35.03 3.45 3.02 4.6 3.05 4.62z"/>
        </svg>
        <span class="pd-dl-text">
          <span>下載 macOS 版</span>
          <span class="pd-dl-sub">Apple Silicon 與 Intel</span>
        </span>
      </a>
      <a class="pd-dl-btn" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M3 5.5L10.4 4.5V11.5H3V5.5ZM11.3 4.4L21 3V11.4H11.3V4.4ZM3 12.5H10.4V19.6L3 18.5V12.5ZM11.3 12.5H21V21L11.4 19.6L11.3 12.5Z"/>
        </svg>
        <span class="pd-dl-text">
          <span>下載 Windows 版</span>
          <span class="pd-dl-sub">.msi 安裝檔</span>
        </span>
      </a>
    </div>
    <p class="pd-trust">全程在你的電腦上執行。免註冊、免上傳、免安裝額外執行環境。</p>

    <div class="pd-demo-wrap">
      <div class="pd-demo-frame">
        <div class="pd-demo-bar">
          <span class="pd-demo-dot red"></span>
          <span class="pd-demo-dot yellow"></span>
          <span class="pd-demo-dot green"></span>
          <span class="pd-demo-title">report.pdf → report.md</span>
        </div>
        <div class="pd-demo-placeholder">示範影片製作中 —— 匯入一份 PDF，馬上得到可編輯的 Markdown。</div>
      </div>
      <p class="pd-demo-caption">實際看看 —— 匯入一份 PDF，馬上得到可編輯的 Markdown。</p>
    </div>
  </div>
</section>

<section id="formats" class="pd-section pd-section-alt">
  <div class="pd-container">
    <div class="pd-section-head">
      <h2>從任何格式匯入</h2>
      <p class="pd-section-sub">
        Pourdown 把檔案轉成 Markdown，並且保留圖片 —— 這是多數轉換工具會直接丟掉的部分。
        標題、清單、表格與連結也一併保留。
      </p>
    </div>
    <div class="pd-format-grid">
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-docx">.docx</div>
        <h3>Word</h3>
        <p>標題、粗體與斜體、巢狀清單、表格、連結與內嵌圖片，全部保留。</p>
      </div>
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-xlsx">.xlsx</div>
        <h3>Excel</h3>
        <p>每個工作表轉為乾淨表格，日期自動格式化，內嵌圖片自動擷取。</p>
      </div>
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-pdf">.pdf</div>
        <h3>PDF</h3>
        <p>自動推斷標題與閱讀順序、重建表格、擷取圖片。（限文字型 PDF。）</p>
      </div>
      <div class="pd-format-card">
        <div class="pd-chip pd-chip-pptx">.pptx</div>
        <h3>PowerPoint</h3>
        <p>投影片標題轉為標題、內文轉為段落、圖片自動擷取。</p>
      </div>
    </div>
    <p class="pd-format-foot">確切保留哪些內容與目前的限制，請見 <a href="/zh/guide/importing">匯入指南 →</a></p>
  </div>
</section>

<section class="pd-section">
  <div class="pd-container">
    <div class="pd-llm-band">
      <div class="pd-llm-inner">
        <div class="pd-llm-kicker">為 Prompting 而生</div>
        <p class="pd-llm-title">用更少的 token，餵給 LLM 更乾淨的內容。</p>
        <p class="pd-llm-sub">餵給語言模型時，Markdown 比原始 PDF 最多可節省 96% 的 token（<a href="/zh/guide/importing">社群基準測試</a>）。轉換一次，往後每次 prompt 都更省。</p>
      </div>
    </div>
  </div>
</section>

<section id="features" class="pd-section pd-section-alt">
  <div class="pd-container">
    <div class="pd-section-head">
      <h2>書寫所需的一切</h2>
      <p class="pd-section-sub">一個專注、不擋路的編輯器。</p>
    </div>
    <div class="pd-feature-grid">
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5Z"/></svg>
        </div>
        <h3>視覺化編輯</h3>
        <p>所見即所得 —— 不必和原始 Markdown 符號搏鬥。</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/></svg>
        </div>
        <h3>保留你的圖片</h3>
        <p>內嵌圖片會被擷取並在文中直接顯示，不會被丟棄。</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        </div>
        <h3>離線且私密</h3>
        <p>所有轉換都在你的裝置上進行，文件不會離開你的電腦。</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><circle cx="7.5" cy="10.5" r="1"/><circle cx="12" cy="7.5" r="1"/><circle cx="16.5" cy="10.5" r="1"/><circle cx="10" cy="15" r="1"/><path d="M12 22a5 5 0 0 1-1-9.9 2 2 0 0 0 1.5-3.1A9.98 9.98 0 0 1 12 2"/></svg>
        </div>
        <h3>多種主題</h3>
        <p>七種內建編輯器主題 —— GitHub Light/Dark、Dracula、Nord、Solarized 等任你挑選。</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
        </div>
        <h3>豐富的 Markdown 語法</h3>
        <p>標題、表格、任務清單、程式碼區塊、引言等 —— 完整支援 GitHub 風格 Markdown。</p>
      </div>
      <div class="pd-feature-card">
        <div class="pd-feature-icon">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z"/></svg>
        </div>
        <h3>中文與英文</h3>
        <p>完整雙語介面 —— 英文與繁體中文，隨時切換。</p>
      </div>
    </div>
  </div>
</section>

<section class="pd-section">
  <div class="pd-container pd-native">
    <h2>原生、輕量、即開即用</h2>
    <p>
      以 Tauri v2 打造，而非 Electron —— 使用系統內建的網頁引擎，而不是打包整個瀏覽器，
      因此下載檔小、開啟快，轉換第一個檔案前也不需安裝任何額外元件。
    </p>
  </div>
</section>

<section class="pd-section pd-section-alt">
  <div class="pd-container pd-cta-band">
    <img src="/logo-64.webp" alt="Pourdown" width="56" height="56">
    <h2>取得 Pourdown</h2>
    <p class="pd-section-sub">下一分鐘就能把你的第一份文件轉成乾淨的 Markdown。</p>
    <div class="pd-dl-row">
      <a class="pd-dl-btn pd-dl-btn-primary" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M16.365 1.43c0 1.14-.493 2.27-1.177 3.08-.744.9-2.02 1.57-3.043 1.57-.12 0-.245-.02-.315-.03-.015-.06-.045-.226-.045-.394 0-1.14.539-2.27 1.222-3.05.723-.83 2.01-1.484 3.032-1.529.03.086.06.212.06.343zm4.243 12.4c-.03.086-.478 1.66-1.58 3.284-.949 1.396-1.933 2.79-3.48 2.816-1.524.03-2.014-.9-3.756-.9-1.744 0-2.286.87-3.728.93-1.492.06-2.63-1.51-3.585-2.9-1.95-2.86-3.442-8.08-1.44-11.607.994-1.75 2.77-2.86 4.702-2.89 1.462-.03 2.843.985 3.756.985.91 0 2.596-1.22 4.377-1.04.746.03 2.84.302 4.185 2.27-.107.067-2.5 1.46-2.47 4.35.03 3.45 3.02 4.6 3.05 4.62z"/>
        </svg>
        <span>下載 macOS 版</span>
      </a>
      <a class="pd-dl-btn" href="https://github.com/passpier/Pourdown/releases/latest">
        <svg class="pd-dl-icon" width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M3 5.5L10.4 4.5V11.5H3V5.5ZM11.3 4.4L21 3V11.4H11.3V4.4ZM3 12.5H10.4V19.6L3 18.5V12.5ZM11.3 12.5H21V21L11.4 19.6L11.3 12.5Z"/>
        </svg>
        <span>下載 Windows 版</span>
      </a>
    </div>
  </div>
</section>

</div>
