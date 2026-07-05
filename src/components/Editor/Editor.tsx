import { useEditor, EditorContent, ReactNodeViewRenderer } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import Typography from '@tiptap/extension-typography';
import Image from '@tiptap/extension-image';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import type { MarkdownSerializerState } from '@tiptap/pm/markdown';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import Table from '@tiptap/extension-table';
import TableRow from '@tiptap/extension-table-row';
import TableHeader from '@tiptap/extension-table-header';
import TableCell from '@tiptap/extension-table-cell';
import { createLowlight, common } from 'lowlight';
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useEditorStore } from '@/stores/editorStore';
import { useEditorLayout } from '@/hooks/useEditorLayout';
import { debounce } from '@/lib/utils';
import { injectFrontmatterAsCodeBlock, restoreFrontmatterFromCodeBlock } from '@/lib/frontmatterUtils';
import { findAnchorHeading } from '@/lib/editorAnchor';
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
  const findBarVisible = useUIStore((state) => state.findBarVisible);
  const setFindBarVisible = useUIStore((state) => state.setFindBarVisible);
  const containerRef = useRef<HTMLDivElement>(null);
  const layoutMetrics = useEditorLayout(containerRef);
  const [hasMeasuredLayout, setHasMeasuredLayout] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [replaceTerm, setReplaceTerm] = useState('');
  const [replaceVisible, setReplaceVisible] = useState(false);
  const document = documents.find(d => d.id === documentId);

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

  // Restore the position (nearest heading, or scroll ratio) left behind when
  // switching from source mode into WYSIWYG mode. Declared after the
  // scroll-reset effect above so it runs later in the same commit and wins
  // over the default reset-to-top. Waits until the editor's content actually
  // reflects this document (frontmatter injection may still be pending).
  const hasRestoredAnchorRef = useRef(false);
  useEffect(() => {
    if (hasRestoredAnchorRef.current) return;
    if (!editor || !document) return;

    const editorMarkdown = (editor.storage['markdown'] as { getMarkdown: () => string }).getMarkdown();
    if (restoreFrontmatterFromCodeBlock(editorMarkdown) !== document.content) return;

    hasRestoredAnchorRef.current = true;

    const anchor = consumePendingAnchor();
    if (!anchor || anchor.documentId !== documentId) return;

    if (anchor.headingIndex >= 0) {
      const headingNodes: { index: number; pos: number; text: string }[] = [];
      let ordinal = -1;
      editor.state.doc.descendants((node, pos) => {
        if (node.type.name === 'heading') {
          ordinal += 1;
          headingNodes.push({ index: ordinal, pos, text: node.textContent });
        }
        return true;
      });

      const target = findAnchorHeading(headingNodes, anchor);

      if (target) {
        editor.chain().focus().setTextSelection(target.pos + 1).scrollIntoView().run();
        return;
      }
    }

    // Fallback: no matching heading, apply the captured scroll ratio.
    if (containerRef.current) {
      const el = containerRef.current;
      const maxScroll = el.scrollHeight - el.clientHeight;
      el.scrollTop = maxScroll > 0 ? anchor.scrollRatio * maxScroll : 0;
    }
  }, [editor, document, documentId, consumePendingAnchor]);

  // Capture the current position when this editor unmounts (i.e. the user
  // switches to source mode), so it can be restored on the way back.
  const editorRef = useRef(editor);
  editorRef.current = editor;
  const documentIdRef = useRef(documentId);
  documentIdRef.current = documentId;
  useEffect(() => {
    return () => {
      const ed = editorRef.current;
      if (!ed) return;

      const selectionFrom = ed.state.selection.from;
      let ordinal = -1;
      let headingIndex = -1;
      let headingText = '';
      ed.state.doc.descendants((node, pos) => {
        if (node.type.name === 'heading') {
          ordinal += 1;
          if (pos <= selectionFrom) {
            headingIndex = ordinal;
            headingText = node.textContent;
          }
        }
        return true;
      });

      let scrollRatio = 0;
      const container = containerRef.current;
      if (container) {
        const maxScroll = container.scrollHeight - container.clientHeight;
        scrollRatio = maxScroll > 0 ? container.scrollTop / maxScroll : 0;
      }

      setPendingAnchor({
        documentId: documentIdRef.current,
        headingIndex,
        headingText,
        scrollRatio,
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
            transition: hasMeasuredLayout ? 'max-width 200ms ease, width 200ms ease' : 'none',
            willChange: hasMeasuredLayout ? 'max-width, width' : 'auto',
          }}
        />
      </div>
      {editor && <TableHoverPanel editor={editor} containerRef={containerRef} />}
    </div>
  );
});
