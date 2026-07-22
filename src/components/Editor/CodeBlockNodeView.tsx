import { NodeViewContent, NodeViewWrapper, type NodeViewProps } from '@tiptap/react';
import { useEffect, useId, useRef, useState } from 'react';
import { normalizeLanguage } from '@/lib/codeBlockUtils';
import { loadKatex, renderMath } from '@/lib/katex';
import { Copy, Check, Trash2, ChevronDown } from 'lucide-react';

type MermaidNamespace = typeof import('mermaid');
type MermaidInstance = import('mermaid').Mermaid;

let mermaidInitialized = false;
let mermaidModule: MermaidNamespace | null = null;

const loadMermaid = async (): Promise<MermaidInstance> => {
  if (!mermaidModule) {
    mermaidModule = await import('mermaid');
  }
  return mermaidModule.default;
};

const COMMON_LANGUAGES: { value: string; label: string }[] = [
  { value: 'plaintext', label: 'Plain Text' },
  { value: 'bash', label: 'Bash' },
  { value: 'c', label: 'C' },
  { value: 'c++', label: 'C++' },
  { value: 'csharp', label: 'C#' },
  { value: 'css', label: 'CSS' },
  { value: 'diff', label: 'Diff' },
  { value: 'dockerfile', label: 'Dockerfile' },
  { value: 'go', label: 'Go' },
  { value: 'graphql', label: 'GraphQL' },
  { value: 'html', label: 'HTML' },
  { value: 'ini', label: 'INI' },
  { value: 'java', label: 'Java' },
  { value: 'javascript', label: 'JavaScript' },
  { value: 'json', label: 'JSON' },
  { value: 'kotlin', label: 'Kotlin' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'math', label: 'Math (LaTeX)' },
  { value: 'mermaid', label: 'Mermaid' },
  { value: 'php', label: 'PHP' },
  { value: 'powershell', label: 'PowerShell' },
  { value: 'python', label: 'Python' },
  { value: 'ruby', label: 'Ruby' },
  { value: 'rust', label: 'Rust' },
  { value: 'scss', label: 'SCSS' },
  { value: 'sql', label: 'SQL' },
  { value: 'swift', label: 'Swift' },
  { value: 'toml', label: 'TOML' },
  { value: 'typescript', label: 'TypeScript' },
  { value: 'xml', label: 'XML' },
  { value: 'yaml', label: 'YAML' },
];

function getLanguageLabel(lang: string): string {
  const found = COMMON_LANGUAGES.find((l) => l.value === lang);
  if (found) return found.label;
  return lang ? lang.charAt(0).toUpperCase() + lang.slice(1) : 'Plain Text';
}

export function CodeBlockNodeView({ node, deleteNode, updateAttributes }: NodeViewProps) {
  const language = normalizeLanguage(node.attrs.language || '');
  const isMermaid = language === 'mermaid';
  const isMath = language === 'math';
  const code = node.textContent ?? '';
  const reactId = useId();
  const renderId = `mermaid-${reactId.replace(/[^a-zA-Z0-9]/g, '')}`;
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isRendering, setIsRendering] = useState(false);
  const mermaidRef = useRef<MermaidInstance | null>(null);
  const [mermaidReady, setMermaidReady] = useState(false);
  const [mathHtml, setMathHtml] = useState<string | null>(null);
  const [mathError, setMathError] = useState<string | null>(null);
  const katexRef = useRef<Awaited<ReturnType<typeof loadKatex>> | null>(null);

  const [hovered, setHovered] = useState(false);
  const [copied, setCopied] = useState(false);
  const [langDropdownOpen, setLangDropdownOpen] = useState(false);
  const langDropdownRef = useRef<HTMLDivElement>(null);
  const copyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!isMermaid) return;

    let cancelled = false;
    const init = async () => {
      try {
        const mermaid = await loadMermaid();
        if (cancelled) return;
        mermaidRef.current = mermaid;
        if (!mermaidInitialized) {
          mermaid.initialize({
            startOnLoad: false,
            securityLevel: 'strict',
            theme: 'neutral',
          });
          mermaidInitialized = true;
        }
        setMermaidReady(true);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'Failed to load mermaid');
        }
      }
    };

    void init();
    return () => {
      cancelled = true;
    };
  }, [isMermaid]);

  useEffect(() => {
    if (!isMermaid || !mermaidReady || !mermaidRef.current) return;

    const trimmed = code.trim();
    let cancelled = false;
    const render = async () => {
      if (trimmed.length === 0) {
        setSvg(null);
        setError(null);
        return;
      }

      setIsRendering(true);
      try {
        const { svg: rendered } = await mermaidRef.current!.render(renderId, trimmed);
        if (!cancelled) {
          setSvg(rendered);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setSvg(null);
          setError(err instanceof Error ? err.message : 'Failed to render diagram');
        }
      } finally {
        if (!cancelled) {
          setIsRendering(false);
        }
      }
    };

    void render();
    return () => {
      cancelled = true;
    };
  }, [code, isMermaid, mermaidReady, renderId]);

  // ```math / ```latex fenced blocks render as display math, sharing the
  // same lazy-loaded KaTeX helper as `MathBlockNodeView`/`MathInlineNodeView`.
  useEffect(() => {
    if (!isMath) return;

    let cancelled = false;
    const trimmed = code.trim();

    const render = async () => {
      if (trimmed.length === 0) {
        setMathHtml(null);
        setMathError(null);
        return;
      }
      try {
        if (!katexRef.current) {
          katexRef.current = await loadKatex();
        }
        if (cancelled) return;
        const result = renderMath(katexRef.current, trimmed, true);
        setMathHtml(result.html);
        setMathError(result.error);
      } catch (err) {
        if (!cancelled) {
          setMathHtml(null);
          setMathError(err instanceof Error ? err.message : 'Failed to load KaTeX');
        }
      }
    };

    void render();
    return () => {
      cancelled = true;
    };
  }, [code, isMath]);

  useEffect(() => {
    if (!langDropdownOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (langDropdownRef.current && !langDropdownRef.current.contains(e.target as Node)) {
        setLangDropdownOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [langDropdownOpen]);

  useEffect(() => {
    return () => {
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current);
    };
  }, []);

  const handleCopy = () => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(true);
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current);
      copyTimeoutRef.current = setTimeout(() => setCopied(false), 1500);
    });
  };

  const toolbarVisible = hovered || langDropdownOpen;

  const toolbar = (
    <div
      contentEditable={false}
      className="absolute top-2 right-2 z-10 flex items-center gap-0.5 rounded-md border bg-background/95 px-1 py-0.5 shadow-sm"
    >
      {/* Language selector */}
      <div className="relative" ref={langDropdownRef}>
        <button
          type="button"
          onClick={() => setLangDropdownOpen((v) => !v)}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
        >
          <span>{getLanguageLabel(language)}</span>
          <ChevronDown className="h-3 w-3 opacity-60" />
        </button>
        {langDropdownOpen && (
          <div className="absolute right-0 top-full z-50 mt-1 max-h-60 min-w-[9rem] overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md">
            {COMMON_LANGUAGES.map((lang) => (
              <button
                key={lang.value}
                type="button"
                onClick={() => {
                  updateAttributes({ language: lang.value });
                  setLangDropdownOpen(false);
                }}
                className={`flex w-full cursor-pointer items-center rounded-sm px-2 py-1.5 text-sm transition-colors hover:bg-accent hover:text-accent-foreground ${
                  language === lang.value ? 'bg-accent text-accent-foreground' : ''
                }`}
              >
                {lang.label}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Divider */}
      <div className="mx-0.5 h-4 w-px bg-border" />

      {/* Copy button */}
      <button
        type="button"
        onClick={handleCopy}
        className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
        aria-label="Copy code"
      >
        {copied ? (
          <Check className="h-3.5 w-3.5 text-green-500" />
        ) : (
          <Copy className="h-3.5 w-3.5" />
        )}
      </button>

      {/* Delete button */}
      <button
        type="button"
        onClick={deleteNode}
        className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-destructive/15 hover:text-destructive"
        aria-label="Delete code block"
      >
        <Trash2 className="h-3.5 w-3.5" />
      </button>
    </div>
  );

  if (!isMermaid && !isMath) {
    return (
      <NodeViewWrapper
        className="tiptap-codeblock"
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        {toolbarVisible && toolbar}
        <pre className={`language-${language}`}>
          <NodeViewContent as="code" />
        </pre>
      </NodeViewWrapper>
    );
  }

  if (isMath) {
    return (
      <NodeViewWrapper
        className="tiptap-codeblock math-block"
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        {toolbarVisible && toolbar}
        <div className="math-block-preview" contentEditable={false}>
          {mathError && (
            <div className="math-fallback" title={mathError}>
              {code}
            </div>
          )}
          {!mathError && mathHtml && (
            <div className="math-katex" dangerouslySetInnerHTML={{ __html: mathHtml }} />
          )}
          {!mathError && !mathHtml && <div className="math-status">Empty equation</div>}
        </div>
        <pre className="language-math math-block-source">
          <NodeViewContent as="code" />
        </pre>
      </NodeViewWrapper>
    );
  }

  return (
    <NodeViewWrapper
      className="tiptap-codeblock tiptap-codeblock-mermaid"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {toolbarVisible && toolbar}
      <div className="mermaid-preview" contentEditable={false}>
        {isRendering && <div className="mermaid-status">Rendering diagram...</div>}
        {!isRendering && error && (
          <div className="mermaid-error">
            <strong>Mermaid error:</strong> {error}
          </div>
        )}
        {!isRendering && !error && svg && (
          <div
            className="mermaid-svg"
            // Mermaid returns sanitized SVG when securityLevel is strict.
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        )}
      </div>
      <pre className="language-mermaid mermaid-source">
        <NodeViewContent as="code" />
      </pre>
    </NodeViewWrapper>
  );
}
