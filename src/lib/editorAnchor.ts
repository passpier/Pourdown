/**
 * Shared helpers for preserving the user's exact reading position when
 * switching between WYSIWYG and source-code editor modes.
 *
 * Both editors render the same markdown, so the Nth "real" heading (i.e. a
 * `#...` line that is not inside a fenced code block, and not inside a YAML
 * frontmatter block) is a stable landmark that exists identically in the
 * ProseMirror doc and in the raw markdown text.
 *
 * Neither editor exposes a source-position for every pixel (tiptap-markdown
 * doesn't retain `sourcepos`), so instead of syncing on raw scroll ratio (which
 * is invalid across two very differently-shaped renderings) we anchor the
 * *viewport-top* to the pair of headings that bracket it, and store the
 * fraction of the way between them. Each mode measures its own two bracketing
 * headings' pixel Y and interpolates, so the mapping stays accurate anywhere
 * in a section, not just at heading boundaries.
 */

import MarkdownIt from 'markdown-it';
import { frontmatterLength } from '@/lib/frontmatterUtils';

// Minimal shape of what we read off a markdown-it token, to avoid fighting
// the library's `export =` typings under this project's ESM interop config.
interface MarkdownItToken {
  type: string;
  tag: string;
  map: [number, number] | null;
  content: string;
  children: MarkdownItToken[] | null;
}

// Configured identically to the `Markdown` tiptap extension in Editor.tsx, so
// this scanner's notion of "heading" always agrees with what Tiptap actually
// renders (fence nesting, HTML blocks, setext headings, etc. all handled by
// the real CommonMark parser rather than an approximation).
const headingScanMd = new MarkdownIt({ html: true, linkify: false, breaks: false });

export interface HeadingInfo {
  /** 0-based ordinal among all "real" (non-fenced, non-frontmatter) headings */
  index: number;
  /** Character offset of the start of the heading line in the source text */
  charOffset: number;
  /** 0-based line number of the heading in the source text */
  line: number;
  /** Heading text (without leading `#`s), used for validation */
  text: string;
  /** Heading level (1-6) */
  level: number;
}

/** A heading landmark together with its measured pixel Y in some editor. */
export interface HeadingLandmark {
  index: number;
  text: string;
  y: number;
}

/** Reference to a single heading landmark, or `null` for a document boundary. */
export interface AnchorBoundary {
  index: number;
  text: string;
}

/**
 * Anchors the viewport-top to the pair of headings that bracket it. `lower`/
 * `upper` are `null` to mean "start of document" / "end of document" so the
 * model covers the whole doc uniformly (before the first heading, after the
 * last, or a heading-less document).
 */
export interface EditorAnchor {
  documentId: string;
  lower: AnchorBoundary | null;
  upper: AnchorBoundary | null;
  /** 0..1 position of the viewport-top between `lower` and `upper`. */
  fraction: number;
}

/**
 * Extract the plain text of a heading's inline token, the same way
 * ProseMirror's `node.textContent` does on the WYSIWYG side: strip emphasis/
 * link/etc. markers and keep only the actual characters. Using
 * `inline.content` instead would keep raw markdown syntax (e.g. `**bold**`),
 * which would never match Tiptap's rendered heading text.
 */
function inlineTokenText(token: MarkdownItToken): string {
  if (!token.children) return token.content;
  let text = '';
  for (const child of token.children) {
    if (child.type === 'text' || child.type === 'code_inline') {
      text += child.content;
    } else if (child.type === 'softbreak' || child.type === 'hardbreak') {
      text += ' ';
    }
  }
  return text;
}

/**
 * Scan raw markdown for headings using the same CommonMark parser
 * (`markdown-it`) that `tiptap-markdown` uses to render the WYSIWYG editor,
 * configured identically (see `headingScanMd` above). This guarantees the
 * source-mode heading list always agrees with the WYSIWYG heading list —
 * previously a hand-rolled regex scanner approximated fence-skipping and
 * would desync on nested fences (e.g. a ```` ```markdown ```` block
 * containing further ``` ``` ``` fences), producing wildly wrong anchors.
 *
 * The leading frontmatter skip matters because the WYSIWYG editor renders
 * frontmatter as a ```` ```yaml ```` code block (see frontmatterUtils.ts), so
 * it must never appear as a heading here either — otherwise the two modes'
 * heading lists disagree by an off-by-one ordinal shift. Feeding the raw
 * leading `---` block to markdown-it directly would also risk it being
 * parsed as a heading/hr, so it's sliced off before parsing, exactly as
 * `injectFrontmatterAsCodeBlock` keeps it out of Tiptap's heading nodes.
 */
export function scanMarkdownHeadings(content: string): HeadingInfo[] {
  const skip = frontmatterLength(content);
  const skippedLines = skip > 0 ? content.slice(0, skip).split('\n').length - 1 : 0;
  const body = content.slice(skip);

  // Char offset of the start of each line within `body`, for mapping a
  // token's `map` (line numbers) back to a character offset.
  const lineStartOffsets: number[] = [0];
  for (let i = 0; i < body.length; i++) {
    if (body[i] === '\n') lineStartOffsets.push(i + 1);
  }

  const tokens = headingScanMd.parse(body, {}) as unknown as MarkdownItToken[];
  const headings: HeadingInfo[] = [];
  let headingIndex = 0;

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token.type !== 'heading_open' || !token.map) continue;

    const inlineToken = tokens[i + 1];
    const text = inlineToken && inlineToken.type === 'inline' ? inlineTokenText(inlineToken) : '';
    const startLine = token.map[0];

    headings.push({
      index: headingIndex++,
      charOffset: skip + (lineStartOffsets[startLine] ?? 0),
      line: skippedLines + startLine,
      text: text.trim(),
      level: Number(token.tag.slice(1)),
    });
  }

  return headings;
}

/** Markdown-it token types that open (or, for self-contained ones, *are*) a
 * top-level block we can use as a landmark. Anything not in this list (list
 * items, table rows/cells, blockquote-internal paragraphs, etc.) is nested
 * inside one of these and doesn't need its own landmark. */
const BLOCK_TOKEN_TYPES = new Set([
  'heading_open',
  'paragraph_open',
  'blockquote_open',
  'bullet_list_open',
  'ordered_list_open',
  'table_open',
  'fence',
  'code_block',
  'hr',
  'html_block',
]);

/** Find the token closing the container opened at `openIndex`. Markdown-it
 * assigns every token a `.level` (nesting depth); everything inside a
 * container is strictly deeper than the container itself, so the next token
 * back at the same level is guaranteed to be its matching close. */
function findMatchingClose(tokens: MarkdownItToken[], openIndex: number): number {
  const level = (tokens[openIndex] as unknown as { level: number }).level;
  for (let i = openIndex + 1; i < tokens.length; i++) {
    if ((tokens[i] as unknown as { level: number }).level === level) return i;
  }
  return tokens.length - 1;
}

/** Flatten the plain text of every `inline` token between two token indices
 * (exclusive of both), the same way `inlineTokenText` flattens a single
 * heading/paragraph. Used to build a text signature for container blocks
 * (blockquote, list, table) whose own token carries no text directly. */
function collectInlineText(tokens: MarkdownItToken[], startIndex: number, endIndex: number): string {
  const parts: string[] = [];
  for (let i = startIndex; i < endIndex; i++) {
    const token = tokens[i];
    if (token.type === 'inline') parts.push(inlineTokenText(token));
  }
  return parts.join(' ');
}

/**
 * Scan raw markdown for every **top-level block** (heading, paragraph,
 * blockquote, list, table, fenced/indented code, thematic break, raw HTML
 * block) rather than just headings. Headings are often too sparse to anchor
 * accurately — a single code-heavy section between two headings can span
 * most of the viewport, and a plain fraction-of-that-segment doesn't track
 * pixel position well when the raw-text and rendered proportions differ a
 * lot (long code fences are a common case). Anchoring between every
 * top-level block instead keeps each interpolated segment small, so drift
 * inside a segment is negligible regardless of content type.
 *
 * Shares `headingScanMd` with `scanMarkdownHeadings` so the two functions
 * always agree on where blocks start; `scanMarkdownHeadings` remains for
 * anywhere only headings are relevant.
 */
export function scanMarkdownBlocks(content: string): HeadingInfo[] {
  const skip = frontmatterLength(content);
  const skippedLines = skip > 0 ? content.slice(0, skip).split('\n').length - 1 : 0;
  const body = content.slice(skip);

  const lineStartOffsets: number[] = [0];
  for (let i = 0; i < body.length; i++) {
    if (body[i] === '\n') lineStartOffsets.push(i + 1);
  }

  const tokens = headingScanMd.parse(body, {}) as unknown as MarkdownItToken[];
  const blocks: HeadingInfo[] = [];
  let blockIndex = 0;

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (!BLOCK_TOKEN_TYPES.has(token.type) || !token.map) continue;
    // Only top-level blocks are independent landmarks; a paragraph nested
    // inside a list item or blockquote is still covered by that container's
    // own landmark, and giving it a separate one would double-count the
    // segment (markdown-it's `.level` is 0 only for genuinely top-level
    // tokens).
    if ((token as unknown as { level: number }).level !== 0) continue;

    const startLine = token.map[0];
    let text: string;
    if (token.type === 'heading_open' || token.type === 'paragraph_open') {
      const inlineToken = tokens[i + 1];
      text = inlineToken && inlineToken.type === 'inline' ? inlineTokenText(inlineToken) : '';
    } else if (token.type === 'fence' || token.type === 'code_block') {
      text = token.content;
    } else if (token.type === 'hr' || token.type === 'html_block') {
      text = '';
    } else {
      // Container block (blockquote/list/table): flatten its own text.
      const closeIndex = findMatchingClose(tokens, i);
      text = collectInlineText(tokens, i + 1, closeIndex);
    }

    blocks.push({
      index: blockIndex++,
      charOffset: skip + (lineStartOffsets[startLine] ?? 0),
      line: skippedLines + startLine,
      text: text.trim().slice(0, 60),
      level: token.type === 'heading_open' ? Number(token.tag.slice(1)) : 0,
    });
  }

  return blocks;
}

/**
 * Find the heading a restored position should target. Matches primarily by
 * heading TEXT (picking whichever match is closest to the captured index, in
 * case of duplicate headings), since the captured index can drift if this
 * module's lightweight scan disagrees with the real parser elsewhere in the
 * document. Falls back to a pure index match if the text isn't found at all
 * (e.g. the heading was edited between capture and restore).
 */
export function findAnchorHeading<T extends { index: number; text: string }>(
  headings: T[],
  anchor: { index: number; text: string }
): T | undefined {
  const textMatches = headings.filter((h) => h.text === anchor.text);
  if (textMatches.length > 0) {
    return textMatches.reduce((best, cur) =>
      Math.abs(cur.index - anchor.index) < Math.abs(best.index - anchor.index) ? cur : best
    );
  }
  return headings.find((h) => h.index === anchor.index);
}

/**
 * Given the list of heading landmarks (with pixel Y already measured in some
 * editor's own geometry) and the current viewport-top Y, find the bracketing
 * pair and express the viewport-top as a fraction between them. Boundaries
 * are `null` to mean the start/end of the document.
 */
export function computeSegmentAnchor(
  headings: HeadingLandmark[],
  viewportTopY: number,
  contentHeight: number
): Omit<EditorAnchor, 'documentId'> {
  const sorted = [...headings].sort((a, b) => a.y - b.y);

  let lowerLandmark: HeadingLandmark | null = null;
  let upperLandmark: HeadingLandmark | null = null;
  for (const h of sorted) {
    if (h.y <= viewportTopY) {
      lowerLandmark = h;
    } else {
      upperLandmark = h;
      break;
    }
  }

  const lowerY = lowerLandmark ? lowerLandmark.y : 0;
  const upperY = upperLandmark ? upperLandmark.y : contentHeight;
  const span = upperY - lowerY;
  const fraction = span > 0 ? Math.min(1, Math.max(0, (viewportTopY - lowerY) / span)) : 0;

  return {
    lower: lowerLandmark ? { index: lowerLandmark.index, text: lowerLandmark.text } : null,
    upper: upperLandmark ? { index: upperLandmark.index, text: upperLandmark.text } : null,
    fraction,
  };
}

/**
 * Inverse of `computeSegmentAnchor`: given the anchor and the (re-measured)
 * heading landmarks in the *restoring* editor's own geometry, resolve the
 * scrollTop to apply. `lower`/`upper` are re-resolved via `findAnchorHeading`
 * (text-first, ordinal fallback) since headings may have shifted between
 * capture and restore.
 */
export function resolveSegmentScrollTop(
  headings: HeadingLandmark[],
  anchor: { lower: AnchorBoundary | null; upper: AnchorBoundary | null; fraction: number },
  contentHeight: number
): number {
  const lowerLandmark = anchor.lower ? findAnchorHeading(headings, anchor.lower) : undefined;
  const upperLandmark = anchor.upper ? findAnchorHeading(headings, anchor.upper) : undefined;

  const lowerY = lowerLandmark ? lowerLandmark.y : 0;
  const upperY = upperLandmark ? upperLandmark.y : contentHeight;
  const span = Math.max(0, upperY - lowerY);

  return Math.max(0, lowerY + anchor.fraction * span);
}

/**
 * Measure the pixel Y (relative to the textarea's content, i.e. what
 * `scrollTop` is measured against) of a set of character offsets within a
 * `<textarea>`, correctly accounting for soft-wrapping. Builds a single
 * off-screen mirror `<div>` that clones the textarea's font/box metrics, then
 * reads `offsetTop` of marker spans spliced at each offset — the standard
 * caret-coordinates technique.
 */
export function measureTextareaLineOffsets(
  textarea: HTMLTextAreaElement,
  content: string,
  charOffsets: number[]
): number[] {
  if (charOffsets.length === 0) return [];

  const style = getComputedStyle(textarea);
  const mirror = document.createElement('div');
  const props: (keyof CSSStyleDeclaration)[] = [
    'boxSizing', 'width', 'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft',
    'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth',
    'fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'letterSpacing',
    'lineHeight', 'tabSize', 'textIndent', 'wordSpacing',
  ];
  for (const prop of props) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (mirror.style as any)[prop] = style[prop] as string;
  }
  mirror.style.position = 'absolute';
  mirror.style.visibility = 'hidden';
  mirror.style.top = '0';
  mirror.style.left = '-9999px';
  mirror.style.height = 'auto';
  mirror.style.whiteSpace = 'pre-wrap';
  mirror.style.wordWrap = 'break-word';
  mirror.style.overflowWrap = 'break-word';
  mirror.style.width = `${textarea.clientWidth}px`;

  // Sort offsets so we can splice markers left-to-right in a single pass.
  const order = charOffsets.map((offset, i) => ({ offset, i })).sort((a, b) => a.offset - b.offset);

  let cursor = 0;
  const frag = document.createDocumentFragment();
  const markers: HTMLSpanElement[] = new Array(charOffsets.length);
  for (const { offset, i } of order) {
    const clamped = Math.max(0, Math.min(content.length, offset));
    if (clamped > cursor) {
      frag.appendChild(document.createTextNode(content.slice(cursor, clamped)));
    }
    const marker = document.createElement('span');
    markers[i] = marker;
    frag.appendChild(marker);
    cursor = clamped;
  }
  if (cursor < content.length) {
    frag.appendChild(document.createTextNode(content.slice(cursor)));
  }
  mirror.appendChild(frag);

  document.body.appendChild(mirror);
  const results = markers.map((marker) => marker.offsetTop);
  document.body.removeChild(mirror);

  return results;
}
