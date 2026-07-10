import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react';
import { useEffect, useRef, useState } from 'react';
import { loadKatex, renderMath } from '@/lib/katex';

/**
 * Inline-math (`$…$`) node view. Renders live KaTeX; clicking it swaps to a
 * small text input bound to the node's `latex` attribute, committing on
 * Enter/blur — the click-to-edit counterpart to `MathBlockNodeView`'s
 * always-editable source line (inline atoms have no ProseMirror content to
 * edit directly, so editing goes through `updateAttributes` instead).
 */
export function MathInlineNodeView({ node, updateAttributes, selected }: NodeViewProps) {
  const latex = (node.attrs.latex as string) || '';
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(latex);
  const [html, setHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const katexRef = useRef<Awaited<ReturnType<typeof loadKatex>> | null>(null);

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
        const result = renderMath(katexRef.current, trimmed, false);
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
    if (editing) inputRef.current?.focus();
  }, [editing]);

  const commit = () => {
    updateAttributes({ latex: draft });
    setEditing(false);
  };

  const startEditing = () => {
    setDraft(latex);
    setEditing(true);
  };

  if (editing) {
    return (
      <NodeViewWrapper as="span" className="math-inline math-inline-editing" contentEditable={false}>
        <input
          ref={inputRef}
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              commit();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              setEditing(false);
            }
          }}
          className="math-inline-input"
        />
      </NodeViewWrapper>
    );
  }

  return (
    <NodeViewWrapper
      as="span"
      className={`math-inline${selected ? ' ProseMirror-selectednode' : ''}`}
      contentEditable={false}
      onClick={startEditing}
    >
      {error && <span className="math-error math-error-inline">{error}</span>}
      {!error && html && <span className="math-katex" dangerouslySetInnerHTML={{ __html: html }} />}
      {!error && !html && <span className="math-status">$…$</span>}
    </NodeViewWrapper>
  );
}
