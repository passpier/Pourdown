import { useEditor, EditorContent, ReactNodeViewRenderer } from '@tiptap/react';
import { Extension } from '@tiptap/core';
import Image from '@tiptap/extension-image';
import { convertFileSrc } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import type { MarkdownSerializerState } from '@tiptap/pm/markdown';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import { createLowlight, common } from 'lowlight';
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useSettingsStore } from '@/stores/settingsStore';
import { useEditorStore } from '@/stores/editorStore';
import { useEditorLayout } from '@/hooks/useEditorLayout';
import { debounce } from '@/lib/utils';
import { injectFrontmatterAsCodeBlock, restoreFrontmatterFromCodeBlock, frontmatterLength } from '@/lib/frontmatterUtils';
import { computeSegmentAnchor, resolveSegmentScrollTop, findAnchorHeading, type HeadingLandmark } from '@/lib/editorAnchor';
import { createMarkdownExtensions } from '@/lib/markdownExtensions';
import { HtmlBlock } from '@/extensions/rawHtml';
import { MathInline, MathBlock } from '@/extensions/math';
import '@/components/CodeBlockRenderer/CodeBlockRenderer.css';
import 'katex/dist/katex.min.css';
import { CodeBlockNodeView } from './CodeBlockNodeView';
import { HtmlBlockNodeView } from './HtmlBlockNodeView';
import { MathInlineNodeView } from './MathInlineNodeView';
import { MathBlockNodeView } from './MathBlockNodeView';
import { SearchExtension, type SearchStorage } from './searchExtension';
import { FindBar } from './FindBar';
import { TableHoverPanel } from './TableHoverPanel';

interface EditorProps {
  documentId: string;
  // Whether this instance is the one currently visible (active document AND
  // active mode). Editor instances now stay mounted across tab/mode switches
  // (see EditorHost) instead of being remounted via a `key` prop, so every
  // side effect that should only apply to the on-screen instance — global
  // editor registration, outline scroll-spy, find sync, scroll-anchor
  // capture/restore — is gated on this flag rather than firing unconditionally
  // on mount.
  active: boolean;
}

/**
 * Splits a path on both `/` and `\` so this works for Windows paths too.
 * Mirrors the identical helper in `documentStore.ts`.
 */
function dirname(path: string): string {
  const parts = path.split(/[/\\]/);
  parts.pop();
  return parts.join('/');
}

export const Editor = memo(function Editor({ documentId, active }: EditorProps) {
  const documents = useDocumentStore((state) => state.documents);
  const updateContent = useDocumentStore((state) => state.updateContent);
  const fontSize = useUIStore((state) => state.fontSize);
  const spellCheck = useSettingsStore((state) => state.spellCheck);
  const setEditor = useEditorStore((state) => state.setEditor);
  const setPendingAnchor = useEditorStore((state) => state.setPendingAnchor);
  const consumePendingAnchor = useEditorStore((state) => state.consumePendingAnchor);
  const setCaptureActiveAnchor = useEditorStore((state) => state.setCaptureActiveAnchor);
  const setActiveHeadingIndex = useEditorStore((state) => state.setActiveHeadingIndex);
  const scrollToHeadingRequest = useEditorStore((state) => state.scrollToHeadingRequest);
  const findBarVisible = useUIStore((state) => state.findBarVisible);
  const setFindBarVisible = useUIStore((state) => state.setFindBarVisible);
  const containerRef = useRef<HTMLDivElement>(null);
  // Live-updated scrollTop, kept in sync on every scroll while this instance
  // is visible (see the outline scroll-spy effect's `onScroll` below). Tab
  // switches hide this pane via `display:none` (EditorHost), which clamps
  // the container's real `scrollTop` to 0 and does not restore it on reveal
  // — reading `container.scrollTop` *after* becoming active again is already
  // too late. This ref is the fallback restore target for that case (see the
  // anchor-restore effect's no-anchor branch): mode switches still use the
  // captured `pendingAnchor` path, which is more precise (segment-based, not
  // a raw pixel offset) and survives layout changes between capture and
  // restore.
  const savedScrollTopRef = useRef(0);
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
  //
  // `document.assetDir` is only set for an imported document that hasn't been
  // saved yet (it's cleared to null by `saveDocument` once media is relocated
  // next to the `.md` — see documentStore.ts). Once that happens, or for any
  // document loaded from disk, relative image paths (`<name>.assets/...`)
  // must be resolved against the document's own directory instead — that's
  // also just standard markdown convention (image paths are relative to the
  // file). Falling back here (rather than repointing `assetDir` itself at the
  // saved directory) keeps `assetDir`'s other meaning intact: `closeDocument`
  // treats a non-null `assetDir` as an orphaned staging dir to delete via
  // `discard_media`, so it must never end up holding the real document folder.
  const assetDirRef = useRef<string | null>(null);
  assetDirRef.current = document?.assetDir ?? (document?.path ? dirname(document.path) : null);

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

  // The base `HtmlBlock` extension (see `src/extensions/rawHtml.ts`) is
  // shared with headless tests and stays React-free; only the live editor
  // needs the sanitized node view, so it's grafted on here — same pattern as
  // `MermaidCodeBlock` above.
  const HtmlBlockWithView = useMemo(() => {
    return HtmlBlock.extend({
      addNodeView() {
        return ReactNodeViewRenderer(HtmlBlockNodeView);
      },
    });
  }, []);

  // Same graft as HtmlBlockWithView above: the base nodes in
  // `extensions/math.ts` stay React-free for headless round-trip tests, and
  // the live editor swaps in the KaTeX-rendering node views here.
  const MathInlineWithView = useMemo(() => {
    return MathInline.extend({
      addNodeView() {
        return ReactNodeViewRenderer(MathInlineNodeView);
      },
    });
  }, []);

  const MathBlockWithView = useMemo(() => {
    return MathBlock.extend({
      addNodeView() {
        return ReactNodeViewRenderer(MathBlockNodeView);
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

  // Links render normally but `Link` is configured with `openOnClick: false`
  // (see markdownExtensions.ts) — a plain click inside a Tauri webview would
  // navigate the whole app window, not open a browser tab. Instead, only a
  // Cmd (macOS) / Ctrl (Windows) + click opens the link, matching the
  // modifier-click convention in VS Code/Obsidian/Typora; a plain click keeps
  // placing the caret so link text stays editable. Only http(s)/mailto are
  // forwarded to the OS — imported/pasted markdown is untrusted, and
  // in-document anchors or relative links have no meaningful desktop target.
  //
  // Implemented as a `handleDOMEvents.click` ProseMirror plugin — the same
  // pattern `FootnoteClickPlugin` (src/extensions/footnotes.ts) already uses
  // for its ref/backref navigation — rather than `editorProps.handleClick`.
  // `handleClick` is PM's synthesized "clean click" callback (gated on
  // posAtCoords/no-drag/selection interplay) and didn't reliably fire for a
  // modifier-click on an inline mark; `handleDOMEvents.click` is a direct
  // passthrough of the native event and modifiers, matching the footnote
  // handler that's proven to work here.
  const LinkClickPlugin = useMemo(() => Extension.create({
    name: 'linkClickHandler',
    addProseMirrorPlugins() {
      return [
        new Plugin({
          key: new PluginKey('linkClickHandler'),
          props: {
            handleDOMEvents: {
              click(_view, event) {
                if (!event.metaKey && !event.ctrlKey) return false;
                const target = event.target as HTMLElement | null;
                const anchor = target?.closest('a[href]');
                const href = anchor?.getAttribute('href');
                if (!href) return false;

                let scheme: string;
                try {
                  scheme = new URL(href).protocol;
                } catch {
                  return false;
                }
                if (scheme !== 'http:' && scheme !== 'https:' && scheme !== 'mailto:') return false;

                event.preventDefault();
                openUrl(href).catch((err) => console.error('Failed to open link:', err));
                return true;
              },
            },
          },
        }),
      ];
    },
  }), []);

  const editor = useEditor({
    extensions: [
      ...createMarkdownExtensions({
        htmlBlock: HtmlBlockWithView,
        mathInline: MathInlineWithView,
        mathBlock: MathBlockWithView,
      }),
      MermaidCodeBlock.configure({
        lowlight,
        defaultLanguage: null, // Null lets Tiptap detect language from markdown info string
        languageClassPrefix: 'language-', // Matches hljs class format: language-javascript, language-python, etc.
      }),
      CustomImage,
      SearchExtension,
      LinkClickPlugin,
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

  // Only the active instance registers itself as *the* global editor (toolbar
  // and native-menu commands act on whichever editor is registered here). On
  // deactivate/unmount, clear the registration only if we're still the one
  // registered — a different instance may have already become active and
  // overwritten it, and we must not clobber that.
  useEffect(() => {
    if (!active || !editor) return;
    setEditor(editor);
    return () => {
      if (useEditorStore.getState().editor === editor) {
        setEditor(null);
      }
    };
  }, [active, editor, setEditor]);

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

  // This instance now stays mounted across both tab switches (kept alive by
  // EditorHost) and mode switches (WYSIWYG <-> source, still separate
  // components but both kept alive per document), so "restore" can no longer
  // be tied to mount — it needs to re-run on every false->true transition of
  // `active` (each time this instance becomes the visible one), and only
  // then. `hasRestoredAnchorRef` tracks that per-activation, reset back to
  // false whenever this instance goes inactive.
  const hasRestoredAnchorRef = useRef(false);
  useEffect(() => {
    if (!active) {
      hasRestoredAnchorRef.current = false;
      return;
    }
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

    // Re-assert a plain pixel scrollTop across a couple of frames, mirroring
    // `apply`'s settle loop above — the reveal-time layout can still shift
    // for a frame or two (see `useEditorLayout`'s width recompute), which
    // would otherwise fight a single synchronous assignment.
    const applyRawScrollTop = (frame: number, target: number, lastScrollTop: number | null) => {
      if (cancelled) return;
      const container = containerRef.current;
      if (!container) return;
      container.scrollTop = target;
      const stable =
        frame >= MIN_SETTLE_FRAMES && lastScrollTop !== null && Math.abs(container.scrollTop - lastScrollTop) < 1;
      if (stable || frame >= MAX_SETTLE_FRAMES) {
        setWidthTransitionEnabled(true);
        return;
      }
      requestAnimationFrame(() => applyRawScrollTop(frame + 1, target, container.scrollTop));
    };

    requestAnimationFrame(() => {
      if (cancelled || hasRestoredAnchorRef.current) return;
      hasRestoredAnchorRef.current = true;

      const pending = useEditorStore.getState().pendingAnchor;
      // Only consume an anchor captured for *this* document — a foreign one
      // (e.g. left over from switching some other document's mode) belongs to
      // that document's own restore and must not be eaten here.
      const anchor = pending && pending.documentId === documentId ? consumePendingAnchor() : null;
      if (!anchor) {
        // No mode-switch anchor for this document — this is a tab switch (or
        // first-ever open). Fall back to the last scrollTop this instance
        // observed while visible (0 on first open, which is a no-op).
        if (savedScrollTopRef.current > 0) {
          applyRawScrollTop(0, savedScrollTopRef.current, null);
        } else {
          setWidthTransitionEnabled(true);
        }
        return;
      }

      apply(0, anchor, null);
    });
    return () => {
      cancelled = true;
    };
  }, [active, editor, document, documentId, consumePendingAnchor, measureBlockLandmarks]);

  // Capture the current position when this instance is about to stop being
  // the visible one — i.e. when the user switches to source mode for this
  // same document (a tab switch away doesn't need this: the DOM subtree just
  // stays mounted-but-hidden, so its native scrollTop is preserved for free).
  //
  // Rather than capturing on unmount (there is no unmount anymore while
  // switching modes — both Editor and SourceEditor instances for a document
  // stay alive), this registers a stable closure in `editorStore` while
  // `active`, which `uiStore`'s editor-mode toggle calls *synchronously*
  // right before flipping the mode — i.e. while this instance's DOM is still
  // visible/laid-out, avoiding the all-zero-rect problem a post-hide
  // measurement would hit.
  const editorRef = useRef(editor);
  editorRef.current = editor;
  const documentIdRef = useRef(documentId);
  documentIdRef.current = documentId;
  const measureBlockLandmarksRef = useRef(measureBlockLandmarks);
  measureBlockLandmarksRef.current = measureBlockLandmarks;

  const captureAnchor = useCallback(() => {
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
  }, [setPendingAnchor]);

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

  // Toggle a `mod-held` class on the editor DOM while Cmd (macOS) / Ctrl
  // (Windows) is held, so the CSS in index.css (`.tiptap.mod-held a:hover`)
  // only shows a pointer cursor over links while the modifier that actually
  // opens them is down — an honest hover affordance for the click handled by
  // `LinkClickPlugin` above, rather than implying a plain click opens links.
  // `blur`/`visibilitychange` are safety nets: releasing the modifier while
  // this window isn't focused (e.g. switching apps with Cmd still down)
  // never fires `keyup` here, so the class would otherwise get stuck on.
  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom;
    const win = window;
    const doc = win.document;

    const setModHeld = (held: boolean) => dom.classList.toggle('mod-held', held);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Meta' || e.key === 'Control') setModHeld(true);
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === 'Meta' || e.key === 'Control') setModHeld(false);
    };
    const reset = () => setModHeld(false);

    win.addEventListener('keydown', onKeyDown);
    win.addEventListener('keyup', onKeyUp);
    win.addEventListener('blur', reset);
    doc.addEventListener('visibilitychange', reset);
    return () => {
      win.removeEventListener('keydown', onKeyDown);
      win.removeEventListener('keyup', onKeyUp);
      win.removeEventListener('blur', reset);
      doc.removeEventListener('visibilitychange', reset);
      dom.classList.remove('mod-held');
    };
  }, [editor]);

  // Apply font size and responsive layout. Font-family is theme-driven via
  // the --font-body CSS var (see index.css .tiptap), not set inline here.
  useEffect(() => {
    if (editor) {
      const editorElement = editor.view.dom;
      editorElement.style.fontSize = `${fontSize}px`;
      editorElement.setAttribute('spellcheck', String(spellCheck));

      // Apply responsive width based on layout metrics
      editorElement.style.maxWidth = `${layoutMetrics.contentWidth}px`;
      editorElement.style.width = '100%';
    }
  }, [fontSize, spellCheck, editor, layoutMetrics.contentWidth]);

  useEffect(() => {
    if (!hasMeasuredLayout && layoutMetrics.contentWidth > 0) {
      setHasMeasuredLayout(true);
    }
  }, [hasMeasuredLayout, layoutMetrics.contentWidth]);

  // Sync search term into Tiptap search extension — only for the visible
  // instance; findBarVisible/searchTerm are global UI state.
  useEffect(() => {
    if (!active) return;
    if (editor && findBarVisible) {
      editor.commands.setSearchTerm(searchTerm);
    } else if (editor && !findBarVisible) {
      editor.commands.setSearchTerm('');
      setSearchTerm('');
    }
  }, [active, searchTerm, findBarVisible, editor]);

  // Close find bar whenever this instance becomes the visible one (tab
  // switch into this document, or mode switch into WYSIWYG for it). Keyed on
  // `active` rather than `documentId` since instances stay mounted now —
  // `documentId` never changes for a given instance, so it would never
  // re-fire past the first mount.
  useEffect(() => {
    if (active) setFindBarVisible(false);
  }, [active, setFindBarVisible]);

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
  // so scrolling stays smooth. Only the visible instance should drive this —
  // a hidden instance's container measures an all-zero rect, which would
  // otherwise clobber the outline highlight with garbage.
  useEffect(() => {
    if (!active) return;
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
      let activeIndex: number | null = null;
      for (const { index, offset } of headingNodes) {
        const dom = editor.view.nodeDOM(offset);
        const el = dom instanceof HTMLElement ? dom : dom?.parentElement;
        if (!el) continue;
        const y = el.getBoundingClientRect().top - containerTop;
        if (y <= 24) {
          activeIndex = index;
        } else {
          break;
        }
      }
      setActiveHeadingIndex(activeIndex);
    };

    const onScroll = () => {
      // Recorded on every event, unthrottled — this is the tab-switch scroll
      // restore fallback (see `savedScrollTopRef`'s doc comment), so it must
      // reflect the position right up to the moment the pane is hidden, not
      // just whichever frame the rAF-throttled heading update lands on.
      savedScrollTopRef.current = container.scrollTop;
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
  }, [active, editor, document?.content, getHeadingNodes, setActiveHeadingIndex]);

  // Outline click-to-scroll: consume a scroll request fired from
  // OutlinePanel and scroll the target heading into view. Only the visible
  // instance should act on it — a hidden instance scrolling itself is just
  // wasted work (and its scrollIntoView could fight the container's native
  // scroll restore when it's later shown again).
  useEffect(() => {
    if (!active) return;
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
    <div className="relative h-full w-full" hidden={!active}>
      {active && findBarVisible && (
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
