/**
 * Lazy KaTeX loader + render helper, mirroring `CodeBlockNodeView.tsx`'s
 * `loadMermaid` — keeps the (sizeable) KaTeX JS out of the startup bundle,
 * fetched only once the editor actually renders a math node.
 */

type KatexNamespace = typeof import('katex');
type KatexModule = KatexNamespace['default'];

let katexModule: KatexModule | null = null;

export async function loadKatex(): Promise<KatexModule> {
  if (!katexModule) {
    const mod = await import('katex');
    katexModule = mod.default;
  }
  return katexModule;
}

export interface MathRenderResult {
  html: string | null;
  error: string | null;
}

/**
 * Renders LaTeX to HTML via the given KaTeX instance. `throwOnError: false`
 * means invalid LaTeX still returns HTML (an inline KaTeX error span) rather
 * than throwing — we surface our own error state instead so the node view can
 * show a consistent error UI (matching the Mermaid error path).
 */
export function renderMath(katex: KatexModule, latex: string, displayMode: boolean): MathRenderResult {
  try {
    const html = katex.renderToString(latex, {
      throwOnError: true,
      displayMode,
      trust: false,
    });
    return { html, error: null };
  } catch (err) {
    return { html: null, error: err instanceof Error ? err.message : 'Failed to render math' };
  }
}
