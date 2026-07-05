import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useEditorStore } from '@/stores/editorStore';
import { useEditorLayout } from '@/hooks/useEditorLayout';
import {
  scanMarkdownHeadings,
  computeSegmentAnchor,
  resolveSegmentScrollTop,
  measureTextareaLineOffsets,
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
  const fontFamily = useUIStore((state) => state.fontFamily);
  const findBarVisible = useUIStore((state) => state.findBarVisible);
  const setFindBarVisible = useUIStore((state) => state.setFindBarVisible);
  const setPendingAnchor = useEditorStore((state) => state.setPendingAnchor);
  const consumePendingAnchor = useEditorStore((state) => state.consumePendingAnchor);
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
  documentIdRef.current = documentId;
  const contentRef = useRef(content);
  contentRef.current = content;

  // Load the document's raw content into the textarea (layout effect so it
  // runs before the anchor-restore effect below, in the same commit).
  useLayoutEffect(() => {
    if (textareaRef.current && doc) {
      textareaRef.current.value = doc.content;
    }
  }, [documentId]);

  // Measure each heading's pixel Y within the textarea's own content flow
  // (accounting for soft-wrapping), producing landmarks comparable to the
  // WYSIWYG editor's own measurements of the *same* headings.
  const measureLandmarks = useCallback((textarea: HTMLTextAreaElement, text: string, headings: ReturnType<typeof scanMarkdownHeadings>): HeadingLandmark[] => {
    const ys = measureTextareaLineOffsets(textarea, text, headings.map((h) => h.charOffset));
    return headings.map((h, i) => ({ index: h.index, text: h.text, y: ys[i] }));
  }, []);

  // Restore the position (fractional viewport-top between two bracketing
  // headings) left behind when switching from WYSIWYG mode into source mode.
  // Runs once on mount only — it should not re-fire when the user simply
  // edits or switches documents.
  useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    const anchor = consumePendingAnchor();
    if (!anchor || anchor.documentId !== documentIdRef.current) return;

    const text = contentRef.current;
    const headings = scanMarkdownHeadings(text);
    const landmarks = measureLandmarks(textarea, text, headings);
    const contentHeight = textarea.scrollHeight;

    textarea.scrollTop = resolveSegmentScrollTop(landmarks, anchor, contentHeight);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Capture the current position when this editor unmounts (i.e. the user
  // switches to WYSIWYG mode), so it can be restored on the way back.
  useLayoutEffect(() => {
    return () => {
      const textarea = textareaRef.current;
      if (!textarea) return;

      const text = contentRef.current;
      const headings = scanMarkdownHeadings(text);
      const landmarks = measureLandmarks(textarea, text, headings);
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
  }, []);

  const handleNext = useCallback(() => {
    if (matchCount === 0) return;
    const next = (currentIndex + 1) % matchCount;
    setCurrentIndex(next);
    scrollToMatch(next);
  }, [currentIndex, matchCount, scrollToMatch]);

  const handlePrev = useCallback(() => {
    if (matchCount === 0) return;
    const prev = (currentIndex - 1 + matchCount) % matchCount;
    setCurrentIndex(prev);
    scrollToMatch(prev);
  }, [currentIndex, matchCount, scrollToMatch]);

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
            fontFamily: fontFamily.includes('mono') ? fontFamily : `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace`,
            maxWidth: `${layoutMetrics.contentWidth}px`,
          }}
          onChange={handleChange}
          spellCheck={false}
          placeholder={t('editor.placeholder')}
        />
      </div>
    </div>
  );
};
