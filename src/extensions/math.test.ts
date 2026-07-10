import { describe, expect, it } from 'vitest';
import { createHeadlessEditor, mdRoundTrip } from '@/lib/markdownTestUtils';

describe('inline math ($…$)', () => {
  it('parses inline math into a mathInline node carrying the raw LaTeX', () => {
    const editor = createHeadlessEditor('Einstein wrote $E=mc^2$ once.');
    const latexValues: string[] = [];
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'mathInline') latexValues.push(node.attrs.latex as string);
    });
    editor.destroy();

    expect(latexValues).toEqual(['E=mc^2']);
  });

  it('round-trips inline math verbatim', () => {
    const md = 'Einstein wrote $E=mc^2$ once.';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('round-trips Greek-letter inline math verbatim', () => {
    const md = 'For example, to show $\\alpha \\beta \\gamma$ inline with other text.';
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('does not treat currency dollar signs as math', () => {
    const md = 'Pay $5 and $10 today.';
    const editor = createHeadlessEditor(md);
    let mathNodes = 0;
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'mathInline') mathNodes += 1;
    });
    editor.destroy();

    expect(mathNodes).toBe(0);
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('does not treat an escaped \\$ as a math delimiter', () => {
    const md = 'Cost: \\$5 flat.';
    const editor = createHeadlessEditor(md);
    let mathNodes = 0;
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'mathInline') mathNodes += 1;
    });
    editor.destroy();

    expect(mathNodes).toBe(0);
  });
});

describe('display math ($$…$$)', () => {
  it('parses a single-line $$ block into a mathBlock node', () => {
    const md = ['$$', 'm=\\frac{b_y-a_y}{b_x-a_x}', '$$'].join('\n');
    const editor = createHeadlessEditor(md);
    const latexValues: string[] = [];
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'mathBlock') latexValues.push(node.textContent);
    });
    editor.destroy();

    expect(latexValues).toEqual(['m=\\frac{b_y-a_y}{b_x-a_x}']);
  });

  it('round-trips a single-line $$ block verbatim', () => {
    const md = ['$$', 'm=\\frac{b_y-a_y}{b_x-a_x}', '$$'].join('\n');
    expect(mdRoundTrip(md).trim()).toBe(md);
  });

  it('parses and round-trips a multi-line $$ block (matrix)', () => {
    const md = [
      '$$',
      'R_x=\\begin{pmatrix}',
      '1 & 0 & 0 & 0\\\\',
      '0 & cos(a) & -sin(a) & 0\\\\',
      '0 & sin(a) & cos(a) & 0\\\\',
      '0 & 0 & 0 & 1',
      '\\end{pmatrix}',
      '$$',
    ].join('\n');

    const editor = createHeadlessEditor(md);
    let blockCount = 0;
    editor.state.doc.descendants((node) => {
      if (node.type.name === 'mathBlock') blockCount += 1;
    });
    editor.destroy();

    expect(blockCount).toBe(1);
    expect(mdRoundTrip(md).trim()).toBe(md);
  });
});

// Note: `mathBlock`'s parseHTML rule is given `priority: 60` so that, inside
// the *live* editor (Editor.tsx), it wins over `CodeBlockLowlight`'s generic
// `pre` rule. The headless pipeline used above (`createMarkdownExtensions`)
// doesn't register a code-block node at all (`codeBlock: false`, matching
// Editor.tsx — the real code block is assembled separately), so that
// precedence can't be exercised here; it's covered by manual verification in
// the editor instead (see the math feature's verification plan).
