import { NodeViewContent, NodeViewWrapper, type NodeViewProps } from '@tiptap/react';
import { useEffect, useRef, useState } from 'react';
import { Trash2 } from 'lucide-react';
import { loadKatex, renderMath } from '@/lib/katex';

/**
 * Display-math (`$$…$$`) node view. Mirrors `CodeBlockNodeView`'s Mermaid
 * branch (live KaTeX preview above an editable LaTeX source), but reveals the
 * source via explicit local state rather than `.ProseMirror-selectednode`:
 * `mathBlock`'s content is regular editable text (like a code block), not an
 * atom, so a click on the `contentEditable={false}` preview doesn't reliably
 * produce a ProseMirror `NodeSelection` the way it can for atomic nodes —
 * verified against a live editor, where the CSS-only approach never toggled.
 * Clicking the rendered equation opens the source; clicking anywhere else in
 * the document (via a document-level listener, same click-outside pattern as
 * `CodeBlockNodeView`'s language dropdown) closes it again.
 */
export function MathBlockNodeView({ node, deleteNode }: NodeViewProps) {
  const latex = node.textContent ?? '';
  const [html, setHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hovered, setHovered] = useState(false);
  const [sourceOpen, setSourceOpen] = useState(false);
  const katexRef = useRef<Awaited<ReturnType<typeof loadKatex>> | null>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    const trimmed = latex.trim();

    const render = async () => {
      if (trimmed.length === 0) {
        setHtml(null);
        setError(null);
        return;
      }
      try {
        if (!katexRef.current) {
          katexRef.current = await loadKatex();
        }
        if (cancelled) return;
        const result = renderMath(katexRef.current, trimmed, true);
        setHtml(result.html);
        setError(result.error);
      } catch (err) {
        if (!cancelled) {
          setHtml(null);
          setError(err instanceof Error ? err.message : 'Failed to load KaTeX');
        }
      }
    };

    void render();
    return () => {
      cancelled = true;
    };
  }, [latex]);

  useEffect(() => {
    if (!sourceOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setSourceOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [sourceOpen]);

  return (
    <NodeViewWrapper
      ref={wrapperRef}
      className="tiptap-codeblock math-block"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {hovered && (
        <div contentEditable={false} className="absolute top-2 right-2 z-10">
          <button
            type="button"
            onClick={deleteNode}
            className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-destructive/15 hover:text-destructive"
            aria-label="Delete math block"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      )}
      <div
        className="math-block-preview"
        contentEditable={false}
        onClick={() => setSourceOpen(true)}
      >
        {error && (
          <div className="math-fallback" title={error}>
            {latex}
          </div>
        )}
        {!error && html && <div className="math-katex" dangerouslySetInnerHTML={{ __html: html }} />}
        {!error && !html && <div className="math-status">Empty equation</div>}
      </div>
      <pre className={`math-block-source${sourceOpen ? ' math-block-source-open' : ''}`}>
        <NodeViewContent as="code" />
      </pre>
    </NodeViewWrapper>
  );
}
