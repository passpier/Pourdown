/**
 * Shared helpers for preserving the user's position (nearest heading + scroll
 * ratio) when switching between WYSIWYG and source-code editor modes.
 *
 * Both editors render the same markdown, so the Nth "real" heading (i.e. a
 * `#...` line that is not inside a fenced code block) is a stable anchor that
 * exists identically in the ProseMirror doc and in the raw markdown text.
 */

export interface HeadingInfo {
  /** 0-based ordinal among all "real" (non-fenced) headings in the document */
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

/**
 * Scan raw markdown for headings, skipping any lines inside fenced code
 * blocks (``` or ~~~) so a `#` comment in a code sample is never mistaken for
 * a heading. Detects both ATX (`# ...`) and Setext (underlined with `===` or
 * `---`) headings, since CommonMark parsers (incl. the one tiptap-markdown
 * uses) treat a bare `---`/`===` line right after a paragraph as a heading,
 * not a horizontal rule.
 *
 * This is a best-effort approximation of CommonMark, not a full parser, so
 * callers should treat the resulting `index` as a hint and prefer matching by
 * `text` (see `findAnchorHeading`) to stay correct even if this scan drifts
 * from the real parser on unusual input.
 */
export function scanMarkdownHeadings(content: string): HeadingInfo[] {
  const headings: HeadingInfo[] = [];
  let inFence = false;
  let fenceMarker = '';
  let charOffset = 0;
  let headingIndex = 0;
  let pendingTextLine: { text: string; charOffset: number; line: number } | null = null;

  const lines = content.split('\n');
  for (let line = 0; line < lines.length; line++) {
    const text = lines[line];
    const fenceMatch = /^\s*(```|~~~)/.exec(text);

    if (fenceMatch) {
      pendingTextLine = null;
      if (!inFence) {
        inFence = true;
        fenceMarker = fenceMatch[1];
      } else if (fenceMatch[1] === fenceMarker) {
        inFence = false;
      }
    } else if (!inFence) {
      const headingMatch = /^(#{1,6})\s+(.*)$/.exec(text);
      const setextMatch = /^(=+|-{2,})\s*$/.exec(text);

      if (headingMatch) {
        headings.push({
          index: headingIndex++,
          charOffset,
          line,
          text: headingMatch[2].trim(),
          level: headingMatch[1].length,
        });
        pendingTextLine = null;
      } else if (setextMatch && pendingTextLine) {
        headings.push({
          index: headingIndex++,
          charOffset: pendingTextLine.charOffset,
          line: pendingTextLine.line,
          text: pendingTextLine.text,
          level: setextMatch[1].startsWith('=') ? 1 : 2,
        });
        pendingTextLine = null;
      } else if (text.trim() === '') {
        pendingTextLine = null;
      } else {
        pendingTextLine = { text: text.trim(), charOffset, line };
      }
    }

    charOffset += text.length + 1; // +1 for the '\n' removed by split
  }

  return headings;
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
  anchor: { headingIndex: number; headingText: string }
): T | undefined {
  const textMatches = headings.filter((h) => h.text === anchor.headingText);
  if (textMatches.length > 0) {
    return textMatches.reduce((best, cur) =>
      Math.abs(cur.index - anchor.headingIndex) < Math.abs(best.index - anchor.headingIndex) ? cur : best
    );
  }
  return headings.find((h) => h.index === anchor.headingIndex);
}

/**
 * Find the ordinal of the last heading at or before the given char offset.
 * Returns -1 if the offset is before the first heading (or there are none).
 */
export function nearestHeadingBeforeOffset(headings: HeadingInfo[], charOffset: number): number {
  let result = -1;
  for (const heading of headings) {
    if (heading.charOffset <= charOffset) {
      result = heading.index;
    } else {
      break;
    }
  }
  return result;
}

/**
 * Estimate a scrollTop that puts the given line near the top of the viewport.
 * Mirrors the estimation previously inlined in SourceEditor's find-in-page.
 */
export function headingLineToScrollTop(line: number, lineHeight: number): number {
  return Math.max(0, line * lineHeight);
}
