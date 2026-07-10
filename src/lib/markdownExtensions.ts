import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import Typography from '@tiptap/extension-typography';
import Table from '@tiptap/extension-table';
import TableRow from '@tiptap/extension-table-row';
import TableHeader from '@tiptap/extension-table-header';
import TableCell from '@tiptap/extension-table-cell';
import Link from '@tiptap/extension-link';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import type { AnyExtension } from '@tiptap/core';
import {
  FootnoteReference,
  FootnotesSection,
  FootnoteDefinition,
  FootnoteClickPlugin,
} from '@/extensions/footnotes';
import {
  HtmlBlock,
  Kbd,
  Highlight,
  Subscript,
  Superscript,
  Underline,
  Abbreviation,
  Small,
} from '@/extensions/rawHtml';
import { MathInline, MathBlock } from '@/extensions/math';

export interface MarkdownExtensionOverrides {
  /**
   * Swap in a React-node-view-backed `htmlBlock` extension (see
   * `Editor.tsx`'s `HtmlBlock.extend({ addNodeView() {...} })`). Defaults to
   * the plain `HtmlBlock` from `rawHtml.ts`, which is fine for headless
   * tests (no DOM rendering asserted there) but renders raw markup as inert
   * text rather than sanitized HTML in a real editor.
   */
  htmlBlock?: AnyExtension;
  /**
   * Swap in a React-node-view-backed `mathInline`/`mathBlock` extension (see
   * `Editor.tsx`'s `MathInline.extend({ addNodeView() {...} })`), same
   * pattern as `htmlBlock` above. Defaults to the plain nodes from
   * `extensions/math.ts`, which render an inert fallback element in headless
   * tests.
   */
  mathInline?: AnyExtension;
  mathBlock?: AnyExtension;
}

/**
 * Extensions that participate in markdown parse/serialize, shared between
 * the live Tiptap editor (`Editor.tsx`) and headless Vitest round-trip tests
 * (`src/extensions/*.test.ts`). Deliberately excludes the three
 * React-coupled pieces that only make sense inside a mounted editor:
 *   - the Mermaid-rendering code-block node view (`MermaidCodeBlock`)
 *   - `CustomImage`'s asset-protocol `renderHTML` (resolves `assetDir` via
 *     `convertFileSrc`, meaningless outside Tauri)
 *   - the search/find-bar highlight extension (`SearchExtension`)
 * `Editor.tsx` appends those after calling this factory. Keeping this as the
 * single source of truth means test coverage exercises the *same* markdown
 * pipeline as production and can't silently drift from it.
 *
 * Note: `codeBlock` is disabled here (matching `Editor.tsx`) since the real
 * code-block node is the lowlight/Mermaid variant assembled in `Editor.tsx`;
 * headless tests accordingly don't cover fenced code blocks.
 */
export function createMarkdownExtensions(overrides: MarkdownExtensionOverrides = {}): AnyExtension[] {
  return [
    StarterKit.configure({
      heading: {
        levels: [1, 2, 3, 4, 5, 6],
      },
      codeBlock: false,
    }),
    Markdown.configure({
      html: true,
      transformPastedText: true,
      transformCopiedText: true,
      // Match Typora/GFM: a single newline within a paragraph renders as a
      // visible line break rather than being collapsed into the same line.
      breaks: true,
      // Auto-detect bare URLs as links at parse time, matching GFM/Typora.
      linkify: true,
    }),
    Typography,
    Table.configure({
      resizable: true,
      handleWidth: 4,
      cellMinWidth: 50,
      lastColumnResizable: true,
      allowTableNodeSelection: false,
    }),
    TableRow,
    TableHeader,
    TableCell,
    // openOnClick disabled: a click inside the Tauri webview would navigate
    // the app window itself away from the editor rather than opening an
    // external browser.
    Link.configure({
      openOnClick: false,
      autolink: true,
      linkOnPaste: true,
    }),
    TaskList,
    TaskItem.configure({
      nested: true,
    }),
    FootnoteReference,
    FootnotesSection,
    FootnoteDefinition,
    FootnoteClickPlugin,
    overrides.htmlBlock ?? HtmlBlock,
    overrides.mathInline ?? MathInline,
    overrides.mathBlock ?? MathBlock,
    Kbd,
    Highlight,
    Subscript,
    Superscript,
    Underline,
    Abbreviation,
    Small,
  ];
}
