/**
 * Utilities for parsing and rendering code blocks with metadata
 */

export interface CodeBlockMetadata {
  language: string;
  filename?: string;
  highlights: number[];
  showLineNumbers: boolean;
}

/**
 * Parse code block info string to extract metadata
 * Format: language filename="name.js" {1,3-5}
 */
export function parseCodeBlockMetadata(infoString: string): CodeBlockMetadata {
  if (!infoString || !infoString.trim()) {
    return {
      language: 'plaintext',
      highlights: [],
      showLineNumbers: true,
    };
  }

  const parts = infoString.trim().split(/\s+/);
  const metadata: CodeBlockMetadata = {
    language: parts[0] || 'plaintext',
    filename: undefined,
    highlights: [],
    showLineNumbers: true,
  };

  // Parse key="value" attributes
  const attrRegex = /(\w+)="([^"]*)"/g;
  let match;
  while ((match = attrRegex.exec(infoString)) !== null) {
    if (match[1] === 'filename') {
      metadata.filename = match[2];
    }
  }

  // Parse line highlights {1,3-5}
  const highlightMatch = infoString.match(/\{([0-9,-]+)\}/);
  if (highlightMatch) {
    metadata.highlights = parseLineHighlights(highlightMatch[1]);
  }

  return metadata;
}

/**
 * Parse line highlight specification
 * "1,3-5,8" -> [1, 3, 4, 5, 8]
 */
export function parseLineHighlights(highlightStr: string): number[] {
  const lines: number[] = [];
  const parts = highlightStr.split(',');

  for (const part of parts) {
    const trimmed = part.trim();
    if (trimmed.includes('-')) {
      const [start, end] = trimmed.split('-').map(Number);
      if (!isNaN(start) && !isNaN(end)) {
        for (let i = start; i <= end; i++) {
          if (!lines.includes(i)) {
            lines.push(i);
          }
        }
      }
    } else {
      const num = Number(trimmed);
      if (!isNaN(num) && !lines.includes(num)) {
        lines.push(num);
      }
    }
  }

  return lines.sort((a, b) => a - b);
}

/**
 * Normalize language identifier to standard form
 */
export function normalizeLanguage(lang: string): string {
  if (!lang) return 'plaintext';

  const languageAliases: Record<string, string> = {
    js: 'javascript',
    ts: 'typescript',
    jsx: 'javascript',
    tsx: 'typescript',
    py: 'python',
    py3: 'python',
    rb: 'ruby',
    sh: 'bash',
    shell: 'bash',
    zsh: 'bash',
    ps: 'powershell',
    ps1: 'powershell',
    cpp: 'c++',
    cc: 'c++',
    cxx: 'c++',
    cs: 'csharp',
    c: 'c',
    java: 'java',
    kt: 'kotlin',
    go: 'go',
    golang: 'go',
    rs: 'rust',
    php: 'php',
    html: 'html',
    htm: 'html',
    css: 'css',
    scss: 'scss',
    sass: 'sass',
    less: 'less',
    json: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    xml: 'xml',
    sql: 'sql',
    mysql: 'mysql',
    postgres: 'postgres',
    postgresql: 'postgres',
    graphql: 'graphql',
    markdown: 'markdown',
    md: 'markdown',
    mermaid: 'mermaid',
    mmd: 'mermaid',
    math: 'math',
    latex: 'math',
    tex: 'math',
    diff: 'diff',
    patch: 'diff',
    dockerfile: 'dockerfile',
    toml: 'toml',
    ini: 'ini',
    r: 'r',
    swift: 'swift',
    kotlin: 'kotlin',
    plaintext: 'plaintext',
    text: 'plaintext',
  };

  const normalized = lang.toLowerCase().trim();
  return languageAliases[normalized] || normalized;
}

/**
 * Check if a language is supported by lowlight
 */
export function isSupportedLanguage(lang: string, supportedLanguages: Set<string>): boolean {
  const normalized = normalizeLanguage(lang);
  return supportedLanguages.has(normalized);
}

/**
 * Extract plain text from code (no syntax highlighting)
 * Useful for copy-to-clipboard or fallback rendering
 */
export function extractPlainText(html: string): string {
  // Remove HTML tags
  return html.replace(/<[^>]*>/g, '');
}

/**
 * Escape HTML special characters
 */
export function escapeHtml(text: string): string {
  const map: Record<string, string> = {
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#039;',
  };
  return text.replace(/[&<>"']/g, (char) => map[char]);
}

/**
 * Generate formatted line number string with padding
 */
export function formatLineNumber(lineNumber: number, totalLines: number): string {
  const maxLength = totalLines.toString().length;
  return lineNumber.toString().padStart(maxLength, ' ');
}
