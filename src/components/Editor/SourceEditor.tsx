import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useSettingsStore } from '@/stores/settingsStore';
import { useEditorStore } from '@/stores/editorStore';
import { useEditorLayout } from '@/hooks/useEditorLayout';
import {
  scanMarkdownBlocks,
  scanMarkdownHeadings,
  computeSegmentAnchor,
  resolveSegmentScrollTop,
  measureTextareaLineOffsets,
  findAnchorHeading,
  type HeadingLandmark,
} from '@/lib/editorAnchor';
import { FindBar } from './FindBar';

interface SourceEditorProps {
  documentId: string;
}

interface SourceMatch {
  start: number;
  end: number;
}

function findMatches(text: string, term: string): SourceMatch[] {
  if (!term) return [];
  const results: SourceMatch[] = [];
  const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const regex = new RegExp(escaped, 'gi');
  let m: RegExpExecArray | null;
  while ((m = regex.exec(text)) !== null) {
    results.push({ start: m.index, end: m.index + m[0].length });
  }
  return results;
}

export const SourceEditor = ({ documentId }: SourceEditorProps) => {
  const { t } = useTranslation();
  const documents = useDocumentStore((state) => state.documents);
  const updateContent = useDocumentStore((state) => state.updateContent);
  const fontSize = useUIStore((state) => state.fontSize);
  const spellCheck = useSettingsStore((state) => state.spellCheck);
  const wordWrap = useSettingsStore((state) => state.wordWrap);
  const findBarVisible = useUIStore((state) => state.findBarVisible);
  const setFindBarVisible = useUIStore((state) => state.setFindBarVisible);
  const setPendingAnchor = useEditorStore((state) => state.setPendingAnchor);
  const consumePendingAnchor = useEditorStore((state) => state.consumePendingAnchor);
  const setActiveHeadingIndex = useEditorStore((state) => state.setActiveHeadingIndex);
  const scrollToHeadingRequest = useEditorStore((state) => state.scrollToHeadingRequest);
  const containerRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const layoutMetrics = useEditorLayout(containerRef);

  const [searchTerm, setSearchTerm] = useState('');
  const [replaceTerm, setReplaceTerm] = useState('');
  const [replaceVisible, setReplaceVisible] = useState(false);
  const [currentIndex, setCurrentIndex] = useState(0);

  const doc = documents.find((d) => d.id === documentId);
  const content = doc?.content ?? '';

  // Keep refs in sync so the unmount-capture cleanup (registered once) always
  // sees the latest documentId/content without needing to re-run per edit.
  const documentIdRef = useRef(documentId);
  const contentRef = useRef(content);
  useLayoutEffect(() => {
    documentIdRef.current = documentId;
    contentRef.current = content;
  }, [documentId, content]);

  // Load the document's raw content into the textarea (layout effect so it
  // runs before the anchor-restore effect below, in the same commit).
  useLayoutEffect(() => {
    if (textareaRef.current && doc) {
      textareaRef.current.value = doc.content;
    }
  }, [documentId]);

  // Measure each top-level block's pixel Y within the textarea's own content
  // flow (accounting for soft-wrapping), producing landmarks comparable to
  // the WYSIWYG editor's own measurements of the *same* blocks. Anchoring on
  // every block rather than just headings keeps interpolated segments small,
  // so drift within a segment stays negligible even across a long code
  // fence between two distant headings.
  const measureLandmarks = useCallback((textarea: HTMLTextAreaElement, text: string, blocks: ReturnType<typeof scanMarkdownBlocks>): HeadingLandmark[] => {
    const ys = measureTextareaLineOffsets(textarea, text, blocks.map((b) => b.charOffset));
    return blocks.map((b, i) => ({ index: b.index, text: b.text, y: ys[i] }));
  }, []);

  // Restore the position (fractional viewport-top between two bracketing
  // headings) left behind when switching from WYSIWYG mode into source mode.
  // Runs once on mount only — it should not re-fire when the user simply
  // edits or switches documents.
  //
  // The textarea's width is driven by `layoutMetrics.contentWidth`, which is
  // computed by `useEditorLayout`'s *passive* effect and is still 0 on this
  // component's first layout-effect pass (mount). Measuring line-wrap
  // against a 0-width textarea collapses every heading to a garbage Y, which
  // is what made the restored position drift/jump to the top on every
  // toggle. So the actual measurement is deferred to a requestAnimationFrame,
  // retrying a frame later if the textarea still hasn't been laid out with a
  // real width yet.
  //
  // Consuming `pendingAnchor` is *also* deferred into that same rAF (instead
  // of reading it synchronously here) because React StrictMode mounts this
  // effect, tears it down, and mounts it again as a purity check. The rAF
  // scheduled by the first (synthetic) pass still fires even though that
  // pass's own cleanup has already set `cancelled = true` — so consuming must
  // be gated on `hasRestoredAnchorRef` (checked *before* consuming, mirroring
  // `Editor.tsx`'s WYSIWYG restore), not on cancellation alone. Otherwise the
  // dead first pass's rAF consumes and discards the anchor before the second,
  // real pass ever gets to see it, and the restore silently never runs.
  const hasRestoredAnchorRef = useRef(false);
  useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    let cancelled = false;

    // Mirrors the WYSIWYG editor's settle loop: re-measure every frame until
    // the target scrollTop stops moving (or a hard cap is hit), instead of
    // trusting a single measurement after a fixed number of retries. The
    // textarea's width is driven by `layoutMetrics.contentWidth`, which is
    // computed by `useEditorLayout`'s *passive* effect and can still be 0 (or
    // mid-transition) for a few frames after mount — measuring line-wrap
    // against that produces a moving target, which is what made the restored
    // position drift/land at the top on every toggle.
    const MAX_SETTLE_FRAMES = 30;
    const MIN_SETTLE_FRAMES = 2;

    const attemptRestore = (frame: number, resolvedAnchor: NonNullable<ReturnType<typeof consumePendingAnchor>>, lastScrollTop: number | null) => {
      if (cancelled) return;
      const ta = textareaRef.current;
      if (!ta) return;
      if (ta.clientWidth === 0) {
        if (frame < MAX_SETTLE_FRAMES) {
          requestAnimationFrame(() => attemptRestore(frame + 1, resolvedAnchor, lastScrollTop));
        }
        return;
      }

      const text = contentRef.current;
      const blocks = scanMarkdownBlocks(text);
      const landmarks = measureLandmarks(ta, text, blocks);
      const contentHeight = ta.scrollHeight;
      const target = resolveSegmentScrollTop(landmarks, resolvedAnchor, contentHeight);

      const stable = frame >= MIN_SETTLE_FRAMES && lastScrollTop !== null && Math.abs(target - lastScrollTop) < 1;
      if (stable || frame >= MAX_SETTLE_FRAMES) {
        // Place the caret at the nearest block at-or-below the restored
        // viewport-top (not the `lower` bracketing landmark, which by
        // construction sits at/above the top and would leave the caret
        // off-screen above the fold).
        const sortedByY = landmarks
          .map((lm, i) => ({ lm, block: blocks[i] }))
          .sort((a, b) => a.lm.y - b.lm.y);
        let caretBlock = sortedByY.find(({ lm }) => lm.y >= target - 1)?.block;
        if (!caretBlock) {
          const lowerLandmark = resolvedAnchor.lower ? findAnchorHeading(landmarks, resolvedAnchor.lower) : undefined;
          caretBlock = lowerLandmark ? blocks.find((b) => b.index === lowerLandmark.index) : undefined;
        }
        // Place the caret before scrolling: focusing the textarea can itself
        // scroll it into view, which would fight the scrollTop set below.
        if (caretBlock) {
          textarea.setSelectionRange(caretBlock.charOffset, caretBlock.charOffset);
        }
        textarea.scrollTop = target;
        textarea.focus({ preventScroll: true });
        return;
      }

      ta.scrollTop = target;
      requestAnimationFrame(() => attemptRestore(frame + 1, resolvedAnchor, target));
    };

    requestAnimationFrame(() => {
      if (cancelled || hasRestoredAnchorRef.current) return;
      hasRestoredAnchorRef.current = true;

      const resolvedAnchor = consumePendingAnchor();
      if (!resolvedAnchor || resolvedAnchor.documentId !== documentIdRef.current) return;
      attemptRestore(0, resolvedAnchor, null);
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Capture the current position when this editor unmounts (i.e. the user
  // switches to WYSIWYG mode), so it can be restored on the way back.
  //
  // Gated on `readyRef`, set one animation frame after mount: React
  // StrictMode's synthetic mount→cleanup→mount cycle runs this cleanup
  // *synchronously*, before any frame has elapsed, so `readyRef.current` is
  // still `false` at that point and the capture is skipped. A real unmount
  // (the user actually switching modes) always happens well after that first
  // frame, so it captures normally. Without this guard, the synthetic
  // cleanup would overwrite the still-pending, not-yet-restored anchor with
  // a bogus one measured from the not-yet-laid-out textarea.
  const readyRef = useRef(false);
  useLayoutEffect(() => {
    readyRef.current = false;
    const raf = requestAnimationFrame(() => {
      readyRef.current = true;
    });
    return () => {
      cancelAnimationFrame(raf);
      const wasReady = readyRef.current;
      readyRef.current = false;
      if (!wasReady) return;

      const textarea = textareaRef.current;
      if (!textarea) return;

      const text = contentRef.current;
      const blocks = scanMarkdownBlocks(text);
      const landmarks = measureLandmarks(textarea, text, blocks);
      const contentHeight = textarea.scrollHeight;

      const anchor = computeSegmentAnchor(landmarks, textarea.scrollTop, contentHeight);
      setPendingAnchor({
        documentId: documentIdRef.current,
        ...anchor,
      });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Close find bar when document changes
  useEffect(() => {
    setFindBarVisible(false);
  }, [documentId, setFindBarVisible]);

  // Cache measured heading Ys per (content, textarea width) so scroll-spy
  // doesn't rebuild the offscreen mirror div (`measureTextareaLineOffsets`)
  // on every scroll tick — only when the text or wrapping actually changed.
  const headingYCacheRef = useRef<{ content: string; width: number; headings: ReturnType<typeof scanMarkdownHeadings>; ys: number[] } | null>(null);
  const getHeadingLandmarks = useCallback((textarea: HTMLTextAreaElement, text: string) => {
    const cache = headingYCacheRef.current;
    if (cache && cache.content === text && cache.width === textarea.clientWidth) {
      return { headings: cache.headings, ys: cache.ys };
    }
    const headings = scanMarkdownHeadings(text);
    const ys = measureTextareaLineOffsets(textarea, text, headings.map((h) => h.charOffset));
    headingYCacheRef.current = { content: text, width: textarea.clientWidth, headings, ys };
    return { headings, ys };
  }, []);

  // Outline scroll-spy: report whichever heading sits at (or just above) the
  // textarea's scrollTop, throttled to at most once per animation frame.
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    let rafId: number | null = null;
    const updateActiveHeading = () => {
      rafId = null;
      const ta = textareaRef.current;
      if (!ta) return;
      const { headings, ys } = getHeadingLandmarks(ta, contentRef.current);
      if (headings.length === 0) {
        setActiveHeadingIndex(null);
        return;
      }
      let active: number | null = null;
      for (let i = 0; i < headings.length; i++) {
        if (ys[i] <= ta.scrollTop + 4) {
          active = headings[i].index;
        } else {
          break;
        }
      }
      setActiveHeadingIndex(active);
    };

    const onScroll = () => {
      if (rafId !== null) return;
      rafId = requestAnimationFrame(updateActiveHeading);
    };

    updateActiveHeading();
    textarea.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      textarea.removeEventListener('scroll', onScroll);
      if (rafId !== null) cancelAnimationFrame(rafId);
      setActiveHeadingIndex(null);
    };
  }, [documentId, content, getHeadingLandmarks, setActiveHeadingIndex]);

  // Outline click-to-scroll: consume a scroll request fired from
  // OutlinePanel and scroll/select the target heading.
  useEffect(() => {
    if (!scrollToHeadingRequest) return;
    const textarea = textareaRef.current;
    if (!textarea) return;
    const { headings, ys } = getHeadingLandmarks(textarea, contentRef.current);
    const targetIdx = headings.findIndex((h) => h.index === scrollToHeadingRequest.index);
    if (targetIdx === -1) return;
    const heading = headings[targetIdx];
    textarea.focus();
    textarea.setSelectionRange(heading.charOffset, heading.charOffset);
    textarea.scrollTop = Math.max(0, ys[targetIdx] - 4);
    // Only the nonce should re-trigger this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollToHeadingRequest?.nonce]);

  const matches = findMatches(content, searchTerm);
  const matchCount = matches.length;

  const scrollToMatch = useCallback((index: number) => {
    const textarea = textareaRef.current;
    if (!textarea || matches.length === 0) return;
    const match = matches[index];
    if (!match) return;
    textarea.focus();
    textarea.setSelectionRange(match.start, match.end);

    // Estimate line height to scroll to the match
    const lineHeight = parseInt(getComputedStyle(textarea).lineHeight || '20', 10);
    const textBefore = content.slice(0, match.start);
    const linesBefore = (textBefore.match(/\n/g) || []).length;
    textarea.scrollTop = linesBefore * lineHeight - textarea.clientHeight / 2;
  }, [matches, content]);

  const handleSearchChange = useCallback((value: string) => {
    setSearchTerm(value);
    setCurrentIndex(0);
  }, [setSearchTerm, setCurrentIndex]);

  const handleNext = useCallback(() => {
    if (matchCount === 0) return;
    const next = (currentIndex + 1) % matchCount;
    setCurrentIndex(next);
    scrollToMatch(next);
  }, [currentIndex, matchCount, scrollToMatch, setCurrentIndex]);

  const handlePrev = useCallback(() => {
    if (matchCount === 0) return;
    const prev = (currentIndex - 1 + matchCount) % matchCount;
    setCurrentIndex(prev);
    scrollToMatch(prev);
  }, [currentIndex, matchCount, scrollToMatch, setCurrentIndex]);

  const handleReplace = useCallback(() => {
    if (matchCount === 0 || !doc) return;
    const match = matches[currentIndex];
    if (!match) return;
    const newContent =
      content.slice(0, match.start) + replaceTerm + content.slice(match.end);
    updateContent(documentId, newContent);
    if (textareaRef.current) {
      textareaRef.current.value = newContent;
    }
    // Move to next match
    const newMatches = findMatches(newContent, searchTerm);
    const newIndex = Math.min(currentIndex, Math.max(0, newMatches.length - 1));
    setCurrentIndex(newIndex);
  }, [matchCount, matches, currentIndex, content, replaceTerm, searchTerm, doc, documentId, updateContent]);

  const handleReplaceAll = useCallback(() => {
    if (matchCount === 0 || !doc) return;
    const escaped = searchTerm.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const newContent = content.replace(new RegExp(escaped, 'gi'), replaceTerm);
    updateContent(documentId, newContent);
    if (textareaRef.current) {
      textareaRef.current.value = newContent;
    }
    setCurrentIndex(0);
  }, [matchCount, searchTerm, replaceTerm, content, doc, documentId, updateContent]);

  const handleCloseFindBar = useCallback(() => {
    setFindBarVisible(false);
    textareaRef.current?.focus();
  }, [setFindBarVisible]);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    updateContent(documentId, e.target.value);
  };

  if (!doc) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        {t('common.no_document_open')}
      </div>
    );
  }

  return (
    <div className="relative h-full w-full">
      {findBarVisible && (
        <FindBar
          searchTerm={searchTerm}
          replaceTerm={replaceTerm}
          matchCount={matchCount}
          currentMatch={matchCount > 0 ? currentIndex + 1 : 0}
          replaceVisible={replaceVisible}
          onSearchChange={handleSearchChange}
          onReplaceChange={setReplaceTerm}
          onNext={handleNext}
          onPrev={handlePrev}
          onReplace={handleReplace}
          onReplaceAll={handleReplaceAll}
          onClose={handleCloseFindBar}
          onToggleReplace={() => setReplaceVisible((v) => !v)}
        />
      )}
      <div
        ref={containerRef}
        className="h-full w-full overflow-hidden flex justify-center px-4 py-6"
      >
        <textarea
          ref={textareaRef}
          className="h-full w-full resize-none bg-transparent focus:outline-none leading-relaxed"
          style={{
            fontSize: `${fontSize}px`,
            fontFamily: 'var(--font-code)',
            maxWidth: `${layoutMetrics.contentWidth}px`,
            whiteSpace: wordWrap ? 'pre-wrap' : 'pre',
            overflowX: wordWrap ? undefined : 'auto',
          }}
          onChange={handleChange}
          spellCheck={spellCheck}
          wrap={wordWrap ? 'soft' : 'off'}
          placeholder={t('editor.placeholder')}
        />
      </div>
    </div>
  );
};
