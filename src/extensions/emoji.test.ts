import { describe, expect, it } from 'vitest';
import { createHeadlessEditor, mdRoundTrip } from '@/lib/markdownTestUtils';

describe('emoji shortcodes', () => {
  it('parses :smile: into the Unicode character', () => {
    const editor = createHeadlessEditor('Have a nice day :smile:!');
    const text = editor.state.doc.textContent;
    editor.destroy();

    expect(text).toContain('😄');
    expect(text).not.toContain(':smile:');
  });

  it('is a one-way conversion: the emoji character persists as plain text, not the shortcode', () => {
    const md = 'Have a nice day 😄!';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('leaves an unknown shortcode as literal text', () => {
    const md = 'Not an emoji: :this-is-not-a-real-shortcode:';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });
});
