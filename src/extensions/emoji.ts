import { Extension } from '@tiptap/core';
import { full as emojiFull } from 'markdown-it-emoji';

interface MarkdownItLike {
  use: (plugin: (md: unknown, options?: unknown) => void) => void;
}

/**
 * Converts `:shortcode:` emoji (e.g. `:smile:`) to the Unicode character at
 * parse time, via `markdown-it-emoji`'s `full` preset (the complete GitHub
 * shortcode set, not just the small `bare` subset). This is one-way: the
 * plugin's own token renderer (`node_modules/markdown-it-emoji/lib/render.mjs`)
 * emits the emoji character itself as the token's rendered output, so the
 * result is indistinguishable from a literal 😄 typed directly — there's no
 * node/mark carrying the original shortcode, and saving a document does not
 * restore `:smile:` from 😄. Matches Typora's behavior and keeps this a
 * plain-text feature with zero schema/serialize surface, unlike the
 * `mathInline`/`footnoteReference` nodes elsewhere in `src/extensions/`.
 *
 * A bare `Extension.create` (no node or mark) is sufficient here — the same
 * pattern `FootnoteClickPlugin` in `footnotes.ts` uses — because
 * tiptap-markdown calls `parse.setup` on any registered extension, not just
 * nodes/marks.
 */
export const EmojiShortcodes = Extension.create({
  name: 'emojiShortcodes',

  addStorage() {
    return {
      markdown: {
        parse: {
          setup: (md: MarkdownItLike) => md.use(emojiFull as unknown as (md: unknown) => void),
        },
      },
    };
  },
});
