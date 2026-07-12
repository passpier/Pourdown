import { describe, expect, it } from 'vitest';
import { createHeadlessEditor, mdRoundTrip } from '@/lib/markdownTestUtils';

describe('definition lists (Term / : Definition shorthand)', () => {
  it('parses a term/definition pair into the schema', () => {
    const md = 'Term\n: Definition text.';
    const editor = createHeadlessEditor(md);
    const types: string[] = [];
    editor.state.doc.descendants((node) => {
      if (['definitionList', 'definitionTerm', 'definitionDescription'].includes(node.type.name)) {
        types.push(node.type.name);
      }
    });
    editor.destroy();

    expect(types).toEqual(['definitionList', 'definitionTerm', 'definitionDescription']);
  });

  it('round-trips a single term/definition verbatim', () => {
    const md = 'Term\n: Definition text.';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('round-trips a term with multiple definitions verbatim', () => {
    const md = 'Coffee\n: A hot beverage.\n: Also a color.';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('round-trips multiple terms verbatim (blank line required between groups)', () => {
    // A blank line between term-groups is required, not optional: without
    // it, CommonMark's lazy-continuation rule absorbs the next term's plain
    // text line into the previous definition's paragraph instead of
    // starting a new term (verified directly against markdown-it-deflist).
    // The list still parses/serializes as one flat `dl`, tight throughout.
    const md = 'Term 1\n: Definition 1.\n\nTerm 2\n: Definition 2.';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('merges a second term into the prior definition if no blank line separates them (documented CommonMark behavior, not this feature)', () => {
    const md = 'Term 1\n: Definition 1.\nTerm 2\n: Definition 2.';
    const editor = createHeadlessEditor(md);
    let terms = 0;
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'definitionTerm') terms++;
    });
    editor.destroy();

    expect(terms).toBe(1);
  });

  it('still routes a literal <dl> HTML block to htmlBlock, unaffected by the shorthand', () => {
    const md = '<dl>\n  <dt>Term</dt>\n  <dd>Definition</dd>\n</dl>';
    const editor = createHeadlessEditor(md);
    let htmlBlocks = 0;
    let definitionLists = 0;
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'htmlBlock') htmlBlocks++;
      if (node.type.name === 'definitionList') definitionLists++;
    });
    editor.destroy();

    expect(htmlBlocks).toBe(1);
    expect(definitionLists).toBe(0);
    expect(mdRoundTrip(md).trim()).toBe(md);
  });
});
