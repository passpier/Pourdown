---
layout: home
title: Pourdown — 匯入 Word、Excel、PDF 與 PowerPoint 的 Markdown 編輯器
description: 將 Word、Excel、PDF、PowerPoint 檔案轉換成乾淨、可編輯 Markdown 的桌面編輯器，並提供即時視覺化（WYSIWYG）編輯。以 Tauri v2、React 與 Rust 打造，免費、開源。
---

<div class="pourdown-home">

<section class="pd-hero">
  <div class="pd-container">
    <h1>將任何文件<br>轉換成乾淨、可編輯的 Markdown</h1>
    <p class="pd-tagline">
      一鍵匯入 Word 文件、試算表、PDF 與簡報，接著用即時視覺化預覽撰寫與編輯。
      免費、離線、開源。
    </p>
    <div class="pd-cta-row">
      <a class="pd-btn pd-btn-primary" href="https://github.com/passpier/Pourdown/releases/latest">
        ⬇ 下載
      </a>
      <a class="pd-btn pd-btn-secondary" href="/zh/guide/getting-started">
        快速開始
      </a>
      <a class="pd-btn pd-btn-secondary" href="https://github.com/passpier/Pourdown">
        前往 GitHub
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
      alt="Pourdown 編輯器畫面，顯示 Markdown 文件、檔案側欄與工具列"
      width="860"
      height="574"
      loading="lazy"
      decoding="async"
    />
  </div>
</section>

<section class="pd-section">
  <div class="pd-container">
    <h2 class="pd-section-title">從任何格式匯入</h2>
    <p class="pd-section-sub">
      Pourdown 會將 Word、Excel、PDF、PowerPoint 檔案轉換成 Markdown，並且
      <strong>保留圖片</strong> —— 這是大多數 Markdown 轉換工具會直接捨棄的
      部分。結構也會一併保留：標題、清單、表格、連結，而不只是純文字。
      （附帶一提，相較於原始 PDF，Markdown 餵給 LLM 時也能節省高達 96% 的
      token 成本 —— <a href="/zh/guide/importing">社群基準測試</a>。）
    </p>

| 格式 | 你會得到什麼 |
|---|---|
| <span class="pd-badge">Word .docx</span> | 標題、粗體/斜體/刪除線、巢狀清單、表格、超連結、內嵌圖片 —— 全部保留 |
| <span class="pd-badge">Excel .xlsx / .ods</span> | 每個工作表轉為乾淨的表格；日期自動格式化；內嵌圖片會被擷取 |
| <span class="pd-badge">PDF</span> | 自動推斷標題與閱讀順序；偵測並重建表格；內嵌圖片會被擷取 |
| <span class="pd-badge">PowerPoint .pptx</span> | 投影片標題轉為標題、內文轉為段落、內嵌圖片會被擷取 |

<p class="pd-section-sub" style="margin-top: 20px; margin-bottom: 0;">
  想知道確切保留哪些內容與目前的已知限制，請參考
  <a href="/zh/guide/importing">匯入文件指南</a>。
</p>

  </div>
</section>

<section class="pd-features">
  <div class="pd-container">
    <h2 class="pd-section-title">撰寫 Markdown 所需的一切</h2>
    <div class="pd-grid">
      <div class="pd-card">
        <div class="pd-card-icon">✏️</div>
        <h3>視覺化編輯</h3>
        <p>透過 Tiptap 驅動的 WYSIWYG 編輯器撰寫，不需處理原始 Markdown 符號。</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">💻</div>
        <h3>原始碼模式</h3>
        <p>隨時切換至原始 Markdown 文字 —— 需要時完全掌控內容。</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🔍</div>
        <h3>尋找與取代</h3>
        <p>文件內搜尋與取代，並可在側欄進行跨檔案搜尋。</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">💾</div>
        <h3>自動儲存</h3>
        <p>你的工作會定期自動儲存 —— 不會遺失編輯內容。</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🎨</div>
        <h3>七種主題</h3>
        <p>GitHub Light/Dark、Dracula、Nord、Solarized —— 挑選你喜歡的外觀。</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🌐</div>
        <h3>中文與英文</h3>
        <p>內建英文與繁體中文介面。</p>
      </div>
      <div class="pd-card">
        <div class="pd-card-icon">🔒</div>
        <h3>離線且私密</h3>
        <p>所有轉換都在你的電腦上完成 —— 你的文件不會被上傳到任何地方。</p>
      </div>
    </div>
  </div>
</section>

<section class="pd-tech">
  <div class="pd-container">
    <h2 class="pd-section-title">原生且輕量</h2>
    <p class="pd-section-sub">
      Pourdown 以 <strong>Tauri v2</strong> 打造，而非 Electron —— 使用作業系統
      內建的網頁引擎，而不是額外打包整個瀏覽器，讓下載檔案與記憶體用量都能保持
      精簡。匯入功能以原生 Rust 實作，不需要另外安裝 Python 之類的執行環境。
      免費、開源，採用 MIT 授權條款。
    </p>
  </div>
</section>

</div>
