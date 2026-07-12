import { describe, expect, it } from 'vitest';
import { createHeadlessEditor, mdRoundTrip } from '@/lib/markdownTestUtils';

describe('raw HTML blocks (htmlBlock)', () => {
  it('captures an arbitrary block HTML tag (<dl>) verbatim as a single leaf node', () => {
    const md = '<dl>\n  <dt>Term</dt>\n  <dd>Definition</dd>\n</dl>';
    const editor = createHeadlessEditor(md);
    const blocks: string[] = [];
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'htmlBlock') blocks.push(node.attrs.html as string);
    });
    editor.destroy();

    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toContain('<dt>Term</dt>');
    expect(blocks[0]).toContain('<dd>Definition</dd>');
  });

  it('round-trips <dl>...</dl> verbatim', () => {
    const md = '<dl>\n  <dt>Term</dt>\n  <dd>Definition</dd>\n</dl>';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('round-trips <details><summary>...</summary>...</details> verbatim', () => {
    const md = '<details>\n  <summary>Click to expand</summary>\n  <p>Hidden content revealed on click.</p>\n</details>';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('is not limited to a fixed tag allowlist — an arbitrary custom block tag round-trips too', () => {
    const md = '<my-custom-widget data-x="1">\n  <p>Not a known HTML5 tag.</p>\n</my-custom-widget>';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });
});

describe('curated inline HTML marks', () => {
  // kbd/u/abbr/small have no shorthand syntax and no custom `markdown.serialize`
  // (see rawHtml.ts), so they still round-trip verbatim through tiptap-markdown's
  // `HTMLMark` fallback.
  const verbatimCases: Array<[label: string, markdown: string]> = [
    ['kbd', '<kbd>Ctrl</kbd>'],
    ['u', '<u>underline</u>'],
    ['abbr', '<abbr title="HyperText Markup Language">HTML</abbr>'],
    ['small', '<small>fine print</small>'],
  ];

  it.each(verbatimCases)('round-trips %s verbatim', (_label, md) => {
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  // mark/sub/sup normalize HTML-tag input to their `==`/`~`/`^` shorthand on
  // save (see the `serialize` override added in rawHtml.ts) — the shorthand
  // itself round-trips to itself.
  const normalizedCases: Array<[label: string, input: string, output: string]> = [
    ['mark', '<mark>highlighted text</mark>', '==highlighted text=='],
    ['sub', 'H<sub>2</sub>O', 'H~2~O'],
    ['sup', 'x<sup>2</sup>', 'x^2^'],
  ];

  it.each(normalizedCases)('normalizes %s HTML input to shorthand on save', (_label, input, output) => {
    expect(mdRoundTrip(input).trim()).toBe(output);
  });

  const shorthandCases: Array<[label: string, markdown: string]> = [
    ['==highlight==', '==highlighted text=='],
    ['~subscript~', 'H~2~O'],
    ['^superscript^', 'x^2^'],
  ];

  it.each(shorthandCases)('round-trips %s shorthand verbatim', (_label, md) => {
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('does not confuse ~subscript~ with ~~strikethrough~~', () => {
    const md = '~~Scratch this.~~ and H~2~O.';
    const editor = createHeadlessEditor(md);
    let strikes = 0;
    let subs = 0;
    editor.state.doc.descendants((node) => {
      if (node.marks.some((m) => m.type.name === 'strike')) strikes++;
      if (node.marks.some((m) => m.type.name === 'subscript')) subs++;
    });
    editor.destroy();

    expect(strikes).toBeGreaterThan(0);
    expect(subs).toBeGreaterThan(0);
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('does not confuse a footnote reference <sup> with the generic superscript mark', () => {
    const editor = createHeadlessEditor('Ref[^a] and x<sup>2</sup>.\n\n[^a]: Def.');
    let footnoteRefs = 0;
    let genericSups = 0;
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'footnoteReference') footnoteRefs++;
      if (node.marks.some((m) => m.type.name === 'superscript')) genericSups++;
    });
    editor.destroy();

    expect(footnoteRefs).toBe(1);
    expect(genericSups).toBeGreaterThan(0);
  });
});
