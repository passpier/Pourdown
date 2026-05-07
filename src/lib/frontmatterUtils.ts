const FRONTMATTER_RE = /^---[ \t]*\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/;
const CODEBLOCK_RE = /^```yaml\n([\s\S]*?)\n```(?:\n|$)/;

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
