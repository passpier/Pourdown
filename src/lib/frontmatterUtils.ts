const FRONTMATTER_RE = /^---[ \t]*\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/;
const CODEBLOCK_RE = /^```yaml\n([\s\S]*?)\n```(?:\n|$)/;

/**
 * Length (in characters) of a leading YAML frontmatter block, including its
 * delimiters and trailing newline, or 0 if the content doesn't start with one.
 * Shared with editorAnchor.ts so heading scans skip frontmatter the same way
 * injectFrontmatterAsCodeBlock() does (frontmatter becomes a code block in the
 * WYSIWYG editor, so it must never be scanned as a heading in source mode).
 */
export function frontmatterLength(content: string): number {
  const m = FRONTMATTER_RE.exec(content);
  return m ? m[0].length : 0;
}

export function injectFrontmatterAsCodeBlock(content: string): string {
  const m = FRONTMATTER_RE.exec(content);
  if (!m) return content;
  return '```yaml\n' + m[1] + '\n```\n' + content.slice(m[0].length);
}

export function restoreFrontmatterFromCodeBlock(content: string): string {
  const m = CODEBLOCK_RE.exec(content);
  if (!m) return content;
  return '---\n' + m[1] + '\n---\n' + content.slice(m[0].length);
}
