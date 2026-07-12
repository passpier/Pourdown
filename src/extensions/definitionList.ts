import { Node, mergeAttributes } from '@tiptap/core';
import type { MarkdownSerializerState } from '@tiptap/pm/markdown';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import deflistPlugin from 'markdown-it-deflist';

interface MarkdownItLike {
  use: (plugin: (md: unknown) => void) => void;
}

/**
 * Definition lists:
 * ```md
 * Term
 * : Definition
 * ```
 * A literal `<dl>...</dl>` typed or pasted as raw HTML already round-trips
 * today via `HtmlBlock`'s `html_block` override (see `rawHtml.ts`) — that
 * override intercepts any CommonMark block-level HTML before it reaches the
 * schema, so it still wins for literal HTML and is untouched by this file.
 * What's added here is the `Term` / `: Definition` *shorthand*, via
 * `markdown-it-deflist`'s block-level `deflist` rule, which emits plain
 * `dl_open`/`dt_open`/`dd_open` tokens with no renderer override needed —
 * markdown-it's default token renderer turns those into literal `<dl>`,
 * `<dt>`, `<dd>` tags (same "no override needed" shape as the inline HTML
 * marks in `rawHtml.ts`), which these nodes' `parseHTML` picks up directly.
 *
 * Scope: `DefinitionDescription`'s content is `inline*`, matching
 * markdown-it-deflist's *tight* list rendering (no blank line between a term
 * and its definition(s), the common case and the only shape this file's
 * serializer produces). A *loose* deflist (blank line inside an item) makes
 * markdown-it-deflist wrap each definition's content in `<p>`, which won't
 * parse into this inline-only schema — same conservative "known limitation,
 * not a bug" trade-off documented elsewhere in this codebase (e.g. PDF table
 * detection). Multi-paragraph definitions aren't supported.
 */
export const DefinitionList = Node.create({
  name: 'definitionList',
  group: 'block',
  content: '(definitionTerm definitionDescription+)+',

  parseHTML() {
    return [{ tag: 'dl' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['dl', mergeAttributes(HTMLAttributes), 0];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownSerializerState, node: ProseMirrorNode) {
          state.renderContent(node);
          state.closeBlock(node);
        },
        parse: {
          // The only node that calls `md.use` for `markdown-it-deflist` —
          // same one-registration-per-plugin convention as `footnotes.ts`.
          setup: (md: MarkdownItLike) => md.use(deflistPlugin as unknown as (md: unknown) => void),
        },
      },
    };
  },
});

export const DefinitionTerm = Node.create({
  name: 'definitionTerm',
  content: 'inline*',

  parseHTML() {
    return [{ tag: 'dt' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['dt', mergeAttributes(HTMLAttributes), 0];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownSerializerState, node: ProseMirrorNode, _parent: ProseMirrorNode, index: number) {
          // A blank line before every term after the first — required, not
          // cosmetic: CommonMark's lazy-continuation rule means a *plain*
          // text line (a second term, with no `:` marker) right after a
          // preceding definition doesn't interrupt that definition's
          // paragraph, so it gets absorbed as a continuation line instead of
          // starting a new term (verified directly against
          // `markdown-it-deflist`: dropping the blank line merges the next
          // term's text into the previous `dd`). `markdown-it-deflist`
          // itself stays tight (no `<p>` wrapping) across this blank line —
          // it only tracks looseness per-item, not at the group boundary —
          // so this doesn't affect the `inline*` content model below.
          if (index > 0) state.write('\n');
          state.renderInline(node);
          // A plain newline (not `closeBlock`, which would insert a blank
          // line via `flushClose`) — the following `: Definition` line must
          // sit immediately below the term with no blank line, or
          // `markdown-it-deflist` stops treating it as one tight list.
          state.ensureNewLine();
        },
        parse: {},
      },
    };
  },
});

export const DefinitionDescription = Node.create({
  name: 'definitionDescription',
  content: 'inline*',

  parseHTML() {
    return [{ tag: 'dd' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['dd', mergeAttributes(HTMLAttributes), 0];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: MarkdownSerializerState, node: ProseMirrorNode) {
          state.write(': ');
          state.renderInline(node);
          // Same reasoning as `DefinitionTerm` above: stay tight so a
          // following `dd` (another definition for the same term) or the
          // next `dt` isn't separated by a blank line. `DefinitionList`'s own
          // `closeBlock` (after all children are rendered) is what produces
          // the blank line *after* the whole list.
          state.ensureNewLine();
        },
        parse: {},
      },
    };
  },
});
