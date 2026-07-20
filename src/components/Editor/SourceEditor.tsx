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
  // See the identical prop doc on `Editor.tsx` — whether this instance is the
  // one currently visible (active document AND active mode). Instances now
  // stay mounted across switches instead of remounting via a `key` prop.
  active: boolean;
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

export const SourceEditor = ({ documentId, active }: SourceEditorProps) => {
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
  const setCaptureActiveAnchor = useEditorStore((state) => state.setCaptureActiveAnchor);
  const setActiveHeadingIndex = useEditorStore((state) => state.setActiveHeadingIndex);
  const scrollToHeadingRequest = useEditorStore((state) => state.scrollToHeadingRequest);
  const containerRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Live-updated scrollTop, kept in sync on every scroll while this instance
  // is visible (see the outline scroll-spy effect's `onScroll` below) — the
  // fallback restore target for a tab switch. See the identical comment on
  // `Editor.tsx`'s `savedScrollTopRef` for why: `display:none` (EditorHost
  // hiding an inactive pane) clamps `scrollTop` to 0, so it must be captured
  // before hiding, not read after becoming active again.
  const savedScrollTopRef = useRef(0);
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
  //
  // Now keyed on `content` (not just `documentId`): instances stay mounted
  // across tab/mode switches (see EditorHost), so a hidden instance's
  // textarea can otherwise go stale if the document is edited from the other
  // mode while this one isn't visible — `documentId` alone would never
  // re-fire past the first mount to pick that up. Guarded by a value check
  // (not just `!==`, which would also true on every render) so a live edit
  // typed into *this* textarea — whose `value` already equals `content` by
  // the time this effect's dependency changes — doesn't get its cursor/
  // selection reset by reassigning `.value` to the same string.
  useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (textarea && doc && textarea.value !== doc.content) {
      textarea.value = doc.content;
    }
  }, [documentId, doc, content]);

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
  // Runs once per false->true transition of `active` — this instance now
  // stays mounted across both tab switches (kept alive by EditorHost) and
  // mode switches (WYSIWYG <-> source, each kept alive per document), so
  // "restore" can no longer be tied to mount. It must not re-fire just
  // because the user edits or the document's content changes while active.
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
    if (!active) {
      hasRestoredAnchorRef.current = false;
      return;
    }
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

    // Re-assert a plain pixel scrollTop across a couple of frames, mirroring
    // `attemptRestore`'s settle loop above — the textarea's width can still
    // be settling for a frame or two after becoming visible.
    const applyRawScrollTop = (frame: number, target: number, lastScrollTop: number | null) => {
      if (cancelled) return;
      const ta = textareaRef.current;
      if (!ta) return;
      ta.scrollTop = target;
      const stable =
        frame >= MIN_SETTLE_FRAMES && lastScrollTop !== null && Math.abs(ta.scrollTop - lastScrollTop) < 1;
      if (stable || frame >= MAX_SETTLE_FRAMES) return;
      requestAnimationFrame(() => applyRawScrollTop(frame + 1, target, ta.scrollTop));
    };

    requestAnimationFrame(() => {
      if (cancelled || hasRestoredAnchorRef.current) return;
      hasRestoredAnchorRef.current = true;

      const pending = useEditorStore.getState().pendingAnchor;
      // Only consume an anchor captured for *this* document — a foreign one
      // belongs to that document's own restore and must not be eaten here.
      const resolvedAnchor =
        pending && pending.documentId === documentIdRef.current ? consumePendingAnchor() : null;
      if (!resolvedAnchor) {
        // No mode-switch anchor for this document — tab switch (or first-ever
        // open). Fall back to the last scrollTop this instance observed while
        // visible (0 on first open, which is a no-op).
        if (savedScrollTopRef.current > 0) {
          applyRawScrollTop(0, savedScrollTopRef.current, null);
        }
        return;
      }
      attemptRestore(0, resolvedAnchor, null);
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  // Capture the current position when this instance is about to stop being
  // the visible one — i.e. when the user switches to WYSIWYG mode for this
  // same document (a tab switch away doesn't need this: the DOM subtree just
  // stays mounted-but-hidden, so its native scrollTop is preserved for free).
  //
  // Rather than capturing on unmount (there is no unmount anymore while
  // switching modes — both Editor and SourceEditor instances for a document
  // stay alive), this registers a stable closure in `editorStore` while
  // `active`, which `uiStore`'s editor-mode toggle calls *synchronously*
  // right before flipping the mode — i.e. while this instance's textarea is
  // still visible/laid-out, avoiding the not-yet-laid-out (0-width) problem
  // a post-hide measurement would hit.
  const captureAnchor = useCallback(() => {
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
  }, [measureLandmarks, setPendingAnchor]);

  useEffect(() => {
    if (!active) return;
    setCaptureActiveAnchor(captureAnchor);
    return () => {
      // Only clear if we're still the registered capturer — a different
      // instance may have already become active and registered its own.
      if (useEditorStore.getState().captureActiveAnchor === captureAnchor) {
        setCaptureActiveAnchor(null);
      }
    };
  }, [active, captureAnchor, setCaptureActiveAnchor]);

  // Close find bar whenever this instance becomes the visible one (tab
  // switch into this document, or mode switch into source for it). Keyed on
  // `active` rather than `documentId` since instances stay mounted now —
  // `documentId` never changes for a given instance, so it would never
  // re-fire past the first mount.
  useEffect(() => {
    if (active) setFindBarVisible(false);
  }, [active, setFindBarVisible]);

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
  // Only the visible instance should drive this — a hidden instance's
  // scrollTop/measurements aren't meaningful to show in the outline.
  useEffect(() => {
    if (!active) return;
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
      let activeIndex: number | null = null;
      for (let i = 0; i < headings.length; i++) {
        if (ys[i] <= ta.scrollTop + 4) {
          activeIndex = headings[i].index;
        } else {
          break;
        }
      }
      setActiveHeadingIndex(activeIndex);
    };

    const onScroll = () => {
      // Recorded on every event, unthrottled — see `savedScrollTopRef`'s doc
      // comment; this is the tab-switch scroll restore fallback, so it must
      // reflect the position right up to the moment the pane is hidden.
      savedScrollTopRef.current = textarea.scrollTop;
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
  }, [active, documentId, content, getHeadingLandmarks, setActiveHeadingIndex]);

  // Outline click-to-scroll: consume a scroll request fired from
  // OutlinePanel and scroll/select the target heading. Only the visible
  // instance should act on it.
  useEffect(() => {
    if (!active) return;
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
    <div className="relative h-full w-full" hidden={!active}>
      {active && findBarVisible && (
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
