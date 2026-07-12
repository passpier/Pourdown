import { defineConfig } from "vitepress";

const ogImage = "https://raw.githubusercontent.com/passpier/Pourdown/main/screenshots/home.png";

export default defineConfig({
  title: "Pourdown",
  base: "/Pourdown/",
  lastUpdated: true,
  cleanUrls: true,

  head: [
    ["link", { rel: "icon", type: "image/png", href: "/Pourdown/favicon.png" }],
    ["link", { rel: "apple-touch-icon", href: "/Pourdown/favicon.png" }],
    [
      "link",
      {
        rel: "preload",
        as: "font",
        type: "font/woff2",
        href: "/Pourdown/fonts/space-grotesk-latin-700.woff2",
        crossorigin: "",
      },
    ],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:image", content: ogImage }],
    ["meta", { property: "og:site_name", content: "Pourdown" }],
    ["meta", { name: "twitter:card", content: "summary_large_image" }],
    ["meta", { name: "twitter:image", content: ogImage }],
    [
      "script",
      { type: "application/ld+json" },
      JSON.stringify({
        "@context": "https://schema.org",
        "@type": "SoftwareApplication",
        name: "Pourdown",
        applicationCategory: "Productivity",
        operatingSystem: "macOS, Windows",
        description:
          "Desktop Markdown editor that converts Word, Excel, PDF, and PowerPoint files into clean, editable Markdown, with live WYSIWYG editing. Built with Tauri v2, React, and Rust.",
        url: "https://passpier.github.io/Pourdown/",
        downloadUrl: "https://github.com/passpier/Pourdown/releases/latest",
        codeRepository: "https://github.com/passpier/Pourdown",
        license: "https://opensource.org/licenses/MIT",
        offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
        keywords:
          "markdown editor, document conversion, Word to markdown, PDF to markdown, WYSIWYG, Tauri, desktop app",
      }),
    ],
  ],

  themeConfig: {
    logo: "/logo-64.webp",
    search: { provider: "local" },
    socialLinks: [{ icon: "github", link: "https://github.com/passpier/Pourdown" }],
  },

  locales: {
    root: {
      label: "English",
      lang: "en",
      description:
        "Desktop Markdown editor that imports Word, Excel, PDF, and PowerPoint files into clean, editable Markdown.",
      themeConfig: {
        nav: [
          { text: "Guide", link: "/guide/getting-started" },
          { text: "Download", link: "https://github.com/passpier/Pourdown/releases/latest" },
          { text: "GitHub", link: "https://github.com/passpier/Pourdown" },
        ],
        sidebar: [
          {
            text: "Guide",
            items: [
              { text: "Getting Started", link: "/guide/getting-started" },
              { text: "Importing Documents", link: "/guide/importing" },
              { text: "Editing", link: "/guide/editing" },
              { text: "Markdown Syntax", link: "/guide/markdown-syntax" },
              { text: "FAQ & Limitations", link: "/guide/faq" },
            ],
          },
        ],
        editLink: {
          pattern: "https://github.com/passpier/Pourdown/edit/main/docs/:path",
          text: "Edit this page on GitHub",
        },
        footer: {
          message: "Released under the MIT License.",
          copyright: "Copyright © 2026 passpier",
        },
      },
    },
    zh: {
      label: "繁體中文",
      lang: "zh-TW",
      description: "將 Word、Excel、PDF、PowerPoint 檔案轉換成乾淨、可編輯 Markdown 的桌面編輯器。",
      themeConfig: {
        nav: [
          { text: "指南", link: "/zh/guide/getting-started" },
          { text: "下載", link: "https://github.com/passpier/Pourdown/releases/latest" },
          { text: "GitHub", link: "https://github.com/passpier/Pourdown" },
        ],
        sidebar: [
          {
            text: "指南",
            items: [
              { text: "快速開始", link: "/zh/guide/getting-started" },
              { text: "匯入文件", link: "/zh/guide/importing" },
              { text: "編輯", link: "/zh/guide/editing" },
              { text: "Markdown 語法", link: "/zh/guide/markdown-syntax" },
              { text: "常見問題與限制", link: "/zh/guide/faq" },
            ],
          },
        ],
        editLink: {
          pattern: "https://github.com/passpier/Pourdown/edit/main/docs/:path",
          text: "在 GitHub 上編輯此頁",
        },
        footer: {
          message: "採用 MIT 授權條款發布。",
          copyright: "Copyright © 2026 passpier",
        },
        docFooter: {
          prev: "上一頁",
          next: "下一頁",
        },
        outlineTitle: "本頁目錄",
        returnToTopLabel: "回到頂部",
        darkModeSwitchLabel: "外觀",
        lastUpdatedText: "最後更新",
      },
    },
  },
});
