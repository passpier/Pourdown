import { Node, mergeAttributes } from '@tiptap/core';
import type { MarkdownSerializerState } from '@tiptap/pm/markdown';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import type Token from 'markdown-it/lib/token.mjs';
import type { RuleInline } from 'markdown-it/lib/parser_inline.mjs';
import type { RuleBlock } from 'markdown-it/lib/parser_block.mjs';
import type StateInline from 'markdown-it/lib/rules_inline/state_inline.mjs';
import type StateBlock from 'markdown-it/lib/rules_block/state_block.mjs';

// markdown-it's own type surface is loose enough (its plugins mutate
// `md.renderer.rules` with untyped functions) that a structural minimal type
// is clearer here than fighting `@types/markdown-it`'s generics — same
// convention as `footnotes.ts`/`rawHtml.ts`.
interface MarkdownItLike {
  inline: { ruler: { after: (afterName: string, name: string, fn: RuleInline) => void } };
  block: {
    ruler: {
      after: (afterName: string, name: string, fn: RuleBlock, opts?: { alt: string[] }) => void;
    };
  };
  renderer: { rules: Record<string, (tokens: Token[], idx: number) => string> };
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * `$…$` delimiter validity check, adapted from the well-known
 * markdown-it-katex dollar-math algorithm: a `$` can only open/close math
 * when it isn't adjacent to whitespace on the "inside" (so `$5 and $10` isn't
 * mistaken for math) and doesn't sit directly against a digit on close (so
 * prose like `$5$` still works but `Price: $5, cost $10` doesn't).
 */
function isValidDelim(state: StateInline, pos: number): { canOpen: boolean; canClose: boolean } {
  const max = state.posMax;
  const prevChar = pos > 0 ? state.src.charCodeAt(pos - 1) : -1;
  const nextChar = pos + 1 <= max ? state.src.charCodeAt(pos + 1) : -1;

  let canOpen = true;
  let canClose = true;

  // ' ' or '\t'
  if (prevChar === 0x20 || prevChar === 0x09) {
    canClose = false;
  }
  if (nextChar === 0x20 || nextChar === 0x09) {
    canOpen = false;
  }

  return { canOpen, canClose };
}

/** Inline `$…$` math. Registered on the inline ruler after `escape` (same slot markdown-it-katex uses). */
function mathInline(state: StateInline, silent: boolean): boolean {
  if (state.src[state.pos] !== '$') return false;

  const openDelim = isValidDelim(state, state.pos);
  if (!openDelim.canOpen) {
    if (!silent) state.pending += '$';
    state.pos += 1;
    return true;
  }

  const start = state.pos + 1;
  let match = start;
  let found = false;

  // Scan forward for a `$` that (a) isn't escaped and (b) is a *valid*
  // closing delimiter, skipping past any candidate that fails either check
  // (e.g. the first `$` in "pay $5 and $10" is preceded by a space, so it
  // can't close — we keep looking rather than bailing out immediately,
  // which is what correctly leaves plain currency text alone).
  while (true) {
    match = state.src.indexOf('$', match);
    if (match === -1) break;

    // Allow escaped `\$` inside the math span: scan backward over any run of
    // backslashes immediately preceding this `$`. An *even* count (including
    // zero) means the `$` itself isn't escaped; an odd count means it is —
    // skip past it and keep scanning for the real delimiter.
    let escapePos = match - 1;
    while (state.src[escapePos] === '\\') escapePos -= 1;
    const backslashCount = match - 1 - escapePos;
    if (backslashCount % 2 === 1) {
      match += 1;
      continue;
    }

    if (match === start) {
      // Empty content — not a valid close, keep scanning.
      match += 1;
      continue;
    }

    if (isValidDelim(state, match).canClose) {
      found = true;
      break;
    }
    match += 1;
  }

  if (!found) {
    if (!silent) state.pending += '$';
    state.pos = start;
    return true;
  }

  if (!silent) {
    const token = state.push('math_inline', 'math', 0);
    token.markup = '$';
    token.content = state.src.slice(start, match);
  }

  state.pos = match + 1;
  return true;
}

/** Display `$$…$$` math, single or multi-line (matrices, `\begin{}` environments). */
function mathBlock(state: StateBlock, start: number, end: number, silent: boolean): boolean {
  let pos = state.bMarks[start] + state.tShift[start];
  let max = state.eMarks[start];

  if (pos + 2 > max || state.src.slice(pos, pos + 2) !== '$$') return false;
  if (silent) return true;

  pos += 2;
  let firstLine = state.src.slice(pos, max);
  let haveEndMarker = false;

  if (firstLine.trim().slice(-2) === '$$') {
    firstLine = firstLine.trim().slice(0, -2);
    haveEndMarker = true;
  }

  let next = start;
  let lastLine = '';
  while (!haveEndMarker) {
    next += 1;
    if (next >= end) break;

    pos = state.bMarks[next] + state.tShift[next];
    max = state.eMarks[next];
    if (pos < max && state.tShift[next] < state.blkIndent) break;

    if (state.src.slice(pos, max).trim().slice(-2) === '$$') {
      const lastLinePos = state.src.slice(0, max).lastIndexOf('$$');
      lastLine = state.src.slice(pos, lastLinePos);
      haveEndMarker = true;
    }
  }

  state.line = next + 1;

  const token = state.push('math_block', 'math', 0);
  token.block = true;
  token.content =
    (firstLine.trim() ? `${firstLine}\n` : '') +
    state.getLines(start + 1, next, state.tShift[start], true) +
    (lastLine.trim() ? lastLine : '');
  token.map = [start, state.line];
  token.markup = '$$';

  return true;
}

/**
 * Registers the dollar-math tokenizer + renderer rules on the shared
 * markdown-it instance. Only `MathInline` registers this as its `parse.setup`
 * hook (mirroring `FootnoteReference` owning the single `md.use` call in
 * `footnotes.ts`) — tiptap-markdown calls `setup` once per registered
 * extension against the *same* instance, so registering twice would run the
 * tokenizer rules twice.
 *
 * Renderer rules emit the raw LaTeX verbatim in schema-friendly wrappers
 * (never rendered KaTeX HTML — that only happens client-side in the node
 * views) so parse/serialize stays React-free and unit-testable:
 *   - inline → `<span data-math-inline="…">` (`encodeURIComponent`-escaped,
 *     same convention as `rawHtml.ts`'s `data-html-block`).
 *   - block → `<pre data-math-block><code>…</code></pre>`, HTML-escaped so it
 *     survives the browser's `DOMParser` as the node's text content, matching
 *     how a real code-block's source text round-trips.
 */
function installMathTokenizer(md: MarkdownItLike) {
  md.inline.ruler.after('escape', 'math_inline', mathInline);
  md.block.ruler.after('blockquote', 'math_block', mathBlock, {
    alt: ['paragraph', 'reference', 'blockquote', 'list'],
  });

  md.renderer.rules.math_inline = (tokens, idx) => {
    const content = tokens[idx].content;
    return `<span data-math-inline="${encodeURIComponent(content)}"></span>`;
  };

  md.renderer.rules.math_block = (tokens, idx) => {
    const content = tokens[idx].content.trim();
    return `<pre data-math-block><code>${escapeHtml(content)}</code></pre>\n`;
  };
}

export const MathInline = Node.create({
  name: 'mathInline',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: false,

  addAttributes() {
    return {
      latex: {
        default: '',
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute('data-math-inline');
          return raw ? decodeURIComponent(raw) : '';
        },
        renderHTML: (attrs: Record<string, unknown>) => ({
          'data-math-inline': encodeURIComponent((attrs.latex as string) || ''),
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-math-inline]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes(HTMLAttributes, { class: 'math-inline-fallback' })];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownSerializerState, node: ProseMirrorNode) {
          state.write(`$${(node.attrs.latex as string) || ''}$`);
        },
        parse: {
          setup: installMathTokenizer,
        },
      },
    };
  },
});

/**
 * Display-math block node. Mirrors a code-block's shape (`content: 'text*'`,
 * `code: true`, `whitespace: 'pre'`) so its LaTeX source is held as editable
 * ProseMirror text, not an opaque attribute — the node view (`MathBlockNodeView`)
 * renders a live KaTeX preview above an editable `<code>` source, same
 * structure as `CodeBlockNodeView`'s Mermaid branch.
 *
 * `parseHTML`'s `pre[data-math-block]` rule is given a higher priority than
 * the plain code-block's generic `pre` rule so it wins the ambiguity — a
 * `<pre data-math-block><code>` produced by `installMathTokenizer` above must
 * not be captured by `CodeBlockLowlight` instead.
 */
export const MathBlock = Node.create({
  name: 'mathBlock',
  group: 'block',
  content: 'text*',
  marks: '',
  code: true,
  defining: true,
  isolating: true,
  whitespace: 'pre',

  parseHTML() {
    return [{ tag: 'pre[data-math-block]', contentElement: 'code', priority: 60 }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['pre', mergeAttributes(HTMLAttributes, { 'data-math-block': '' }), ['code', {}, 0]];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownSerializerState, node: ProseMirrorNode) {
          state.write('$$\n');
          state.text(node.textContent, false);
          state.write('\n$$');
          state.closeBlock(node);
        },
        parse: {},
      },
    };
  },
});
