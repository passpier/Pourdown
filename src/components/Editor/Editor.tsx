import { useEditor, EditorContent, ReactNodeViewRenderer } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import Typography from '@tiptap/extension-typography';
import Image from '@tiptap/extension-image';
import { convertFileSrc } from '@tauri-apps/api/core';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import type { MarkdownSerializerState } from '@tiptap/pm/markdown';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import Table from '@tiptap/extension-table';
import TableRow from '@tiptap/extension-table-row';
import TableHeader from '@tiptap/extension-table-header';
import TableCell from '@tiptap/extension-table-cell';
import { createLowlight, common } from 'lowlight';
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useEditorStore } from '@/stores/editorStore';
import { useEditorLayout } from '@/hooks/useEditorLayout';
import { debounce } from '@/lib/utils';
import { injectFrontmatterAsCodeBlock, restoreFrontmatterFromCodeBlock, frontmatterLength } from '@/lib/frontmatterUtils';
import { computeSegmentAnchor, resolveSegmentScrollTop, findAnchorHeading, type HeadingLandmark } from '@/lib/editorAnchor';
import '@/components/CodeBlockRenderer/CodeBlockRenderer.css';
import { CodeBlockNodeView } from './CodeBlockNodeView';
import { SearchExtension, type SearchStorage } from './searchExtension';
import { FindBar } from './FindBar';
import { TableHoverPanel } from './TableHoverPanel';

interface EditorProps {
  documentId: string;
}

export const Editor = memo(function Editor({ documentId }: EditorProps) {
  const documents = useDocumentStore((state) => state.documents);
  const updateContent = useDocumentStore((state) => state.updateContent);
  const fontSize = useUIStore((state) => state.fontSize);
  const fontFamily = useUIStore((state) => state.fontFamily);
  const setEditor = useEditorStore((state) => state.setEditor);
  const setPendingAnchor = useEditorStore((state) => state.setPendingAnchor);
  const consumePendingAnchor = useEditorStore((state) => state.consumePendingAnchor);
  const setActiveHeadingIndex = useEditorStore((state) => state.setActiveHeadingIndex);
  const scrollToHeadingRequest = useEditorStore((state) => state.scrollToHeadingRequest);
  const findBarVisible = useUIStore((state) => state.findBarVisible);
  const setFindBarVisible = useUIStore((state) => state.setFindBarVisible);
  const containerRef = useRef<HTMLDivElement>(null);
  const layoutMetrics = useEditorLayout(containerRef);
  const [hasMeasuredLayout, setHasMeasuredLayout] = useState(false);
  // Kept false until the anchor-restore settle loop finishes (or, if there's
  // nothing to restore, until the mount effect confirms that). Enabling the
  // `max-width` CSS transition before that would animate the content width
  // while restore is still re-measuring landmarks against a moving layout —
  // exactly the timing bug that made the restored scroll position land
  // somewhere different on every toggle. Once enabled it only ever fires on
  // genuine width changes (window resize, sidebar toggle), which is the
  // actual intent of the transition.
  const [widthTransitionEnabled, setWidthTransitionEnabled] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [replaceTerm, setReplaceTerm] = useState('');
  const [replaceVisible, setReplaceVisible] = useState(false);
  const document = documents.find(d => d.id === documentId);

  // Read synchronously during render (not in an effect) so it's guaranteed to
  // be current before the content-sync effect below calls setContent — that's
  // when CustomImage's renderHTML resolves image `src`s against this dir.
  const assetDirRef = useRef<string | null>(null);
  assetDirRef.current = document?.assetDir ?? null;

  // Create lowlight instance with a smaller default language set
  // 'common' covers popular languages while keeping bundle size smaller
  const lowlight = useMemo(() => createLowlight(common), []);

  const MermaidCodeBlock = useMemo(() => {
    return CodeBlockLowlight.extend({
      addNodeView() {
        return ReactNodeViewRenderer(CodeBlockNodeView);
      },
    });
  }, []);

  const CustomImage = useMemo(() =>
    Image.extend({
      inline: true,
      group: 'inline',

      addAttributes() {
        return {
          ...this.parent?.(),
          style: {
            default: null,
            parseHTML: (el: Element) => el.getAttribute('style'),
            renderHTML: (attrs: Record<string, unknown>) =>
              attrs.style ? { style: attrs.style } : {},
          },
        };
      },

      // Sidecar images from import are stored as a *relative* markdown path
      // (`assets/image1.png`, resolved against the active document's
      // `assetDir`) so the .md source stays portable. For on-screen display
      // only, rewrite that relative path to a `convertFileSrc()` asset:// URL
      // that the webview can actually load; the stored node attrs (and thus
      // the markdown serializer above) keep the original relative path.
      renderHTML({ node, HTMLAttributes }) {
        const src = HTMLAttributes.src as string | undefined;
        const dir = assetDirRef.current;
        if (src && dir && !/^[a-z][a-z0-9+.-]*:/i.test(src) && !src.startsWith('//')) {
          HTMLAttributes = { ...HTMLAttributes, src: convertFileSrc(`${dir}/${src}`) };
        }
        return this.parent?.({ node, HTMLAttributes }) ?? ['img', HTMLAttributes];
      },

      addStorage() {
        return {
          markdown: {
            serialize(state: MarkdownSerializerState, node: ProseMirrorNode) {
              if (node.attrs.style) {
                const src = node.attrs.src as string || '';
                const alt = node.attrs.alt ? ` alt="${node.attrs.alt as string}"` : '';
                state.write(`<img src="${src}"${alt} style="${node.attrs.style as string}">`);
              } else {
                const alt = state.esc((node.attrs.alt as string) || '');
                const src = state.esc(node.attrs.src as string);
                const title = node.attrs.title
                  ? ` "${(node.attrs.title as string).replace(/"/g, '\\"')}"`
                  : '';
                state.write(`![${alt}](${src}${title})`);
              }
            },
          },
        };
      },
    }), []);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: {
          levels: [1, 2, 3, 4, 5, 6],
        },
        codeBlock: false, // Disable default code block to use lowlight
      }),
      MermaidCodeBlock.configure({
        lowlight,
        defaultLanguage: null, // Null lets Tiptap detect language from markdown info string
        languageClassPrefix: 'language-', // Matches hljs class format: language-javascript, language-python, etc.
      }),
      Markdown.configure({
        html: true,
        transformPastedText: true,
        transformCopiedText: true,
      }),
      Typography,
      CustomImage,
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
      SearchExtension,
    ],
    content: document?.content || '',
    editorProps: {
      attributes: {
        class: 'tiptap prose prose-sm sm:prose lg:prose-lg xl:prose-lg focus:outline-none w-full max-w-4xl',
        spellcheck: 'true',
      },
    },
    onUpdate: debounce(({ editor }) => {
      const markdown = (editor.storage['markdown'] as { getMarkdown: () => string }).getMarkdown();
      updateContent(documentId, restoreFrontmatterFromCodeBlock(markdown));
    }, 500),
  });

  useEffect(() => {
    setEditor(editor ?? null);
    return () => setEditor(null);
  }, [editor, setEditor]);

  // Update editor content when document changes
  useEffect(() => {
    if (editor && document) {
      const editorMarkdown = (editor.storage['markdown'] as { getMarkdown: () => string }).getMarkdown();
      if (restoreFrontmatterFromCodeBlock(editorMarkdown) !== document.content) {
        console.log('📄 Loading document content:', document.content.substring(0, 100) + '...');
        editor.commands.clearContent();
        editor.commands.setContent(injectFrontmatterAsCodeBlock(document.content));
        console.log('✅ Document content loaded');
      }
    }
  }, [document?.content, editor, documentId]);

  // Reset scroll position when switching documents
  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
  }, [documentId]);

  // Measure every *top-level* block's (not just headings') pixel Y in
  // container-scroll coordinates. Anchoring only between headings leaves
  // huge, inaccurate segments on code-heavy documents (a long fence between
  // two headings occupies a very different proportion of raw text vs.
  // rendered height); anchoring between every top-level block keeps each
  // interpolated segment small so drift within a segment is negligible.
  //
  // When the document has a leading YAML frontmatter block, it's injected as
  // the first top-level node (see `injectFrontmatterAsCodeBlock`) but has no
  // counterpart in `scanMarkdownBlocks` (which skips it deliberately, see
  // that function's docs) — skip it here too so both sides' ordinals agree.
  const measureBlockLandmarks = useCallback((ed: NonNullable<typeof editor>, container: HTMLDivElement): HeadingLandmark[] => {
    const containerTop = container.getBoundingClientRect().top - container.scrollTop;
    const skipFirst = document ? frontmatterLength(document.content) > 0 : false;
    const landmarks: HeadingLandmark[] = [];
    let childIndex = -1;
    let ordinal = -1;
    ed.state.doc.forEach((node, offset) => {
      childIndex += 1;
      if (skipFirst && childIndex === 0) return;
      ordinal += 1;
      const dom = ed.view.nodeDOM(offset);
      const el = dom instanceof HTMLElement ? dom : dom?.parentElement;
      const y = el ? el.getBoundingClientRect().top - containerTop : 0;
      landmarks.push({ index: ordinal, text: node.textContent.slice(0, 60), y });
    });
    return landmarks;
  }, [document]);

  // Restore the position (fractional viewport-top between two bracketing
  // headings) left behind when switching from source mode into WYSIWYG mode.
  // Declared after the scroll-reset effect above so it runs later in the same
  // commit and wins over the default reset-to-top.
  //
  // This used to gate on `restoreFrontmatterFromCodeBlock(editor.getMarkdown())
  // === document.content` to wait for the "load content" effect above to
  // finish. But Tiptap's markdown round-trip normalises whitespace/escaping/
  // fences, so for documents with raw HTML, escaped characters, or nested
  // fences that equality is never byte-exact — the gate silently never
  // passed, restore never ran, and the mount's reset-to-top effect won by
  // default. The editor already receives `document.content` as its initial
  // `content` (see `useEditor` above), so there's nothing to wait for here;
  // only the `hasRestoredAnchorRef` guard is needed to run this once.
  //
  // What *does* still need waiting for is async layout: code-block
  // highlighting and the `CodeBlockNodeView` React node views reflow after
  // mount, shifting heading Y positions for a few frames on code-heavy docs.
  // So the restore re-measures and re-applies scrollTop across a few frames
  // instead of trusting a single measurement.
  //
  // `pendingAnchor` is consumed *inside* the deferred rAF, not synchronously
  // in the effect body, and `hasRestoredAnchorRef` is only flipped once that
  // consumption actually happens. Reason: React StrictMode mounts this
  // effect, tears it down, and mounts it again as a purity check. The
  // synthetic teardown cancels this run's rAF chain before it ever fires
  // (`cancelled = true`), so if consumption happened synchronously up front,
  // the anchor would be gone and `hasRestoredAnchorRef` would already be
  // true by the second (real) mount — permanently skipping the restore.
  // Deferring both means the synthetic pass's rAF is cancelled before doing
  // anything, and the second, real mount performs the one-and-only restore.
  // How many frames the settle loop is willing to keep re-measuring before
  // giving up and finalizing wherever it last landed (~500ms at 60fps) — a
  // safety net against a doc that never stops reflowing (e.g. a very slow
  // async Mermaid render), not the normal exit path.
  const MAX_SETTLE_FRAMES = 30;
  // Require at least this many measurements before a match can be declared
  // stable, so an accidental same-value hit on frame 1 doesn't end the loop
  // before layout (width transition, code-block highlighting) has even
  // started moving.
  const MIN_SETTLE_FRAMES = 2;

  const hasRestoredAnchorRef = useRef(false);
  useEffect(() => {
    if (hasRestoredAnchorRef.current) return;
    if (!editor || !document) return;

    let cancelled = false;

    const finalizeCaret = (
      landmarks: HeadingLandmark[],
      anchor: NonNullable<ReturnType<typeof consumePendingAnchor>>,
      container: HTMLDivElement
    ) => {
      // Place the caret at the actual visible top of the content (not at the
      // bracketing landmark, which by construction sits at or above the
      // viewport-top and would land the caret off-screen above the fold).
      const rect = container.getBoundingClientRect();
      const coords = { left: rect.left + rect.width / 2, top: rect.top + 4 };
      const hit = editor.view.posAtCoords(coords);
      if (hit) {
        editor.commands.setTextSelection(hit.pos);
      } else {
        // Fallback for the rare case posAtCoords misses (e.g. viewport-top
        // sits over a non-text node): use the lower bracketing landmark.
        // Ordinals here must skip the injected-frontmatter node exactly like
        // `measureBlockLandmarks` does, or they'd point at the wrong block.
        const lowerTarget = anchor.lower ? findAnchorHeading(landmarks, anchor.lower) : undefined;
        if (lowerTarget) {
          const skipFirst = document ? frontmatterLength(document.content) > 0 : false;
          let childIndex = -1;
          let ordinal = -1;
          editor.state.doc.forEach((_node, offset) => {
            childIndex += 1;
            if (skipFirst && childIndex === 0) return;
            ordinal += 1;
            if (ordinal === lowerTarget.index) {
              editor.commands.setTextSelection(offset + 1);
            }
          });
        }
      }
      editor.commands.focus(undefined, { scrollIntoView: false });
      setWidthTransitionEnabled(true);
    };

    const apply = (
      frame: number,
      anchor: NonNullable<ReturnType<typeof consumePendingAnchor>>,
      lastScrollTop: number | null
    ) => {
      if (cancelled) return;
      const container = containerRef.current;
      if (!container) return;

      const landmarks = measureBlockLandmarks(editor, container);
      const contentHeight = container.scrollHeight;
      const target = resolveSegmentScrollTop(landmarks, anchor, contentHeight);
      container.scrollTop = target;

      const stable =
        frame >= MIN_SETTLE_FRAMES && lastScrollTop !== null && Math.abs(target - lastScrollTop) < 1;
      if (stable || frame >= MAX_SETTLE_FRAMES) {
        finalizeCaret(landmarks, anchor, container);
        return;
      }
      requestAnimationFrame(() => apply(frame + 1, anchor, target));
    };

    requestAnimationFrame(() => {
      if (cancelled || hasRestoredAnchorRef.current) return;
      hasRestoredAnchorRef.current = true;

      const anchor = consumePendingAnchor();
      if (!anchor || anchor.documentId !== documentId) {
        // Nothing to restore (first-ever open of this document) — safe to
        // enable the width transition immediately.
        setWidthTransitionEnabled(true);
        return;
      }

      apply(0, anchor, null);
    });
    return () => {
      cancelled = true;
    };
  }, [editor, document, documentId, consumePendingAnchor, measureBlockLandmarks]);

  // Capture the current position when this editor unmounts (i.e. the user
  // switches to source mode), so it can be restored on the way back.
  const editorRef = useRef(editor);
  editorRef.current = editor;
  const documentIdRef = useRef(documentId);
  documentIdRef.current = documentId;
  const measureBlockLandmarksRef = useRef(measureBlockLandmarks);
  measureBlockLandmarksRef.current = measureBlockLandmarks;
  // useLayoutEffect (not useEffect) is required here: its cleanup runs
  // synchronously during the commit, before React detaches this subtree's
  // DOM. A plain useEffect's cleanup fires asynchronously after the DOM is
  // already removed, at which point getBoundingClientRect() on the container
  // and every heading returns an all-zero rect, collapsing every landmark to
  // the same Y and producing a garbage anchor.
  //
  // Gated on `captureReadyRef`, set one animation frame after mount: React
  // StrictMode mounts every component, tears it down, and mounts it again as
  // a purity check, and that synthetic teardown runs this cleanup
  // *synchronously* — before any frame has elapsed and before the anchor
  // restore effect above has actually applied its scroll position. Without
  // this guard, that synthetic cleanup would capture the not-yet-restored
  // (or not-yet-laid-out) state and overwrite a still-pending, correct
  // anchor with a bogus one. A real unmount (the user switching modes) always
  // happens well after that first frame, so it captures normally.
  const captureReadyRef = useRef(false);
  useLayoutEffect(() => {
    captureReadyRef.current = false;
    const raf = requestAnimationFrame(() => {
      captureReadyRef.current = true;
    });
    return () => {
      cancelAnimationFrame(raf);
      const wasReady = captureReadyRef.current;
      captureReadyRef.current = false;
      if (!wasReady) return;

      const ed = editorRef.current;
      const container = containerRef.current;
      if (!ed || !container) return;

      const landmarks = measureBlockLandmarksRef.current(ed, container);
      const contentHeight = container.scrollHeight;
      const anchor = computeSegmentAnchor(landmarks, container.scrollTop, contentHeight);

      setPendingAnchor({
        documentId: documentIdRef.current,
        ...anchor,
      });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Apply font settings and responsive layout
  useEffect(() => {
    if (editor) {
      const editorElement = editor.view.dom;
      editorElement.style.fontSize = `${fontSize}px`;
      editorElement.style.fontFamily = fontFamily;
      
      // Apply responsive width based on layout metrics
      editorElement.style.maxWidth = `${layoutMetrics.contentWidth}px`;
      editorElement.style.width = '100%';
    }
  }, [fontSize, fontFamily, editor, layoutMetrics.contentWidth]);

  useEffect(() => {
    if (!hasMeasuredLayout && layoutMetrics.contentWidth > 0) {
      setHasMeasuredLayout(true);
    }
  }, [hasMeasuredLayout, layoutMetrics.contentWidth]);

  // Sync search term into Tiptap search extension
  useEffect(() => {
    if (editor && findBarVisible) {
      editor.commands.setSearchTerm(searchTerm);
    } else if (editor && !findBarVisible) {
      editor.commands.setSearchTerm('');
      setSearchTerm('');
    }
  }, [searchTerm, findBarVisible, editor]);

  // Close find bar when document changes
  useEffect(() => {
    setFindBarVisible(false);
  }, [documentId, setFindBarVisible]);

  // Every heading node in document order, paired with its ProseMirror
  // position, keyed by a 0-based ordinal across only headings — this matches
  // `scanMarkdownHeadings`'s ordinal (both walk document order and skip
  // everything that isn't a "real" heading), so an OutlinePanel row's index
  // addresses the same heading here.
  const getHeadingNodes = useCallback((ed: NonNullable<typeof editor>) => {
    const nodes: { index: number; offset: number }[] = [];
    let index = -1;
    ed.state.doc.forEach((node, offset) => {
      if (node.type.name === 'heading') {
        index += 1;
        nodes.push({ index, offset });
      }
    });
    return nodes;
  }, []);

  // Outline scroll-spy: report whichever heading sits at (or just above) the
  // container's viewport-top, throttled to at most once per animation frame
  // so scrolling stays smooth.
  useEffect(() => {
    const container = containerRef.current;
    if (!editor || !container) return;

    let rafId: number | null = null;
    const updateActiveHeading = () => {
      rafId = null;
      const headingNodes = getHeadingNodes(editor);
      if (headingNodes.length === 0) {
        setActiveHeadingIndex(null);
        return;
      }
      const containerTop = container.getBoundingClientRect().top;
      let active: number | null = null;
      for (const { index, offset } of headingNodes) {
        const dom = editor.view.nodeDOM(offset);
        const el = dom instanceof HTMLElement ? dom : dom?.parentElement;
        if (!el) continue;
        const y = el.getBoundingClientRect().top - containerTop;
        if (y <= 24) {
          active = index;
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
    container.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      container.removeEventListener('scroll', onScroll);
      if (rafId !== null) cancelAnimationFrame(rafId);
      setActiveHeadingIndex(null);
    };
  }, [editor, document?.content, getHeadingNodes, setActiveHeadingIndex]);

  // Outline click-to-scroll: consume a scroll request fired from
  // OutlinePanel and scroll the target heading into view.
  useEffect(() => {
    if (!editor || !scrollToHeadingRequest) return;
    const headingNodes = getHeadingNodes(editor);
    const target = headingNodes.find((h) => h.index === scrollToHeadingRequest.index);
    if (!target) return;
    const dom = editor.view.nodeDOM(target.offset);
    const el = dom instanceof HTMLElement ? dom : dom?.parentElement;
    el?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    // Only the nonce should re-trigger this; editor/getHeadingNodes are
    // stable identities we don't want to re-run on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollToHeadingRequest?.nonce]);

  const matchCount = (editor?.storage['search'] as SearchStorage | undefined)?.results?.length ?? 0;
  const currentMatch = matchCount > 0 ? ((editor?.storage['search'] as SearchStorage | undefined)?.currentIndex ?? 0) + 1 : 0;

  const handleSearchChange = useCallback((value: string) => {
    setSearchTerm(value);
  }, []);

  const handleReplaceChange = useCallback((value: string) => {
    setReplaceTerm(value);
  }, []);

  const handleNext = useCallback(() => {
    editor?.commands.findNext();
  }, [editor]);

  const handlePrev = useCallback(() => {
    editor?.commands.findPrev();
  }, [editor]);

  const handleReplace = useCallback(() => {
    editor?.commands.replaceCurrentMatch(replaceTerm);
  }, [editor, replaceTerm]);

  const handleReplaceAll = useCallback(() => {
    editor?.commands.replaceAllMatches(replaceTerm);
  }, [editor, replaceTerm]);

  const handleCloseFindBar = useCallback(() => {
    setFindBarVisible(false);
    editor?.commands.focus();
  }, [setFindBarVisible, editor]);

  const handleToggleReplace = useCallback(() => {
    setReplaceVisible((v) => !v);
  }, []);

  if (!document) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        No document selected
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
          currentMatch={currentMatch}
          replaceVisible={replaceVisible}
          onSearchChange={handleSearchChange}
          onReplaceChange={handleReplaceChange}
          onNext={handleNext}
          onPrev={handlePrev}
          onReplace={handleReplace}
          onReplaceAll={handleReplaceAll}
          onClose={handleCloseFindBar}
          onToggleReplace={handleToggleReplace}
        />
      )}
      <div
        ref={containerRef}
        className="h-full w-full overflow-auto flex justify-center px-4 py-6"
        data-layout-metrics={JSON.stringify(layoutMetrics)}
      >
        <EditorContent
          editor={editor}
          className="h-full"
          style={{
            width: '100%',
            maxWidth: hasMeasuredLayout ? `${layoutMetrics.contentWidth}px` : undefined,
            transition:
              hasMeasuredLayout && widthTransitionEnabled
                ? 'max-width 200ms ease, width 200ms ease'
                : 'none',
            willChange: hasMeasuredLayout ? 'max-width, width' : 'auto',
          }}
        />
      </div>
      {editor && <TableHoverPanel editor={editor} containerRef={containerRef} />}
    </div>
  );
});
