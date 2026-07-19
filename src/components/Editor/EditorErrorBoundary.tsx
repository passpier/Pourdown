import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';
import { SourceEditor } from './SourceEditor';
import { useDocumentStore } from '@/stores/documentStore';

interface Props {
  documentId: string;
  // Bumping this (the document's own content) re-arms the boundary: editing
  // the raw markdown in the fallback below — which is how a user fixes
  // whatever triggered the crash — lets the rich editor re-mount and retry
  // on the next content change, without requiring an app restart.
  resetKey: string;
  // Plain string, not react-i18next's `WithTranslation` HOC: that HOC's
  // `getDerivedStateFromProps` typing doesn't unify with this class's own
  // (TS2345), so translation is resolved by the functional wrapper below via
  // `useTranslation()` and passed down as a already-resolved string instead.
  fallbackMessage: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
  resetKey: string;
}

/**
 * Contains a crash in one document's rich (WYSIWYG) editor to that
 * document's own pane instead of taking down the whole app.
 *
 * Why this exists: `EditorHost` keeps every recently-visited document's
 * `<Editor>` mounted (see its doc comment), so a document whose content
 * makes Tiptap/tiptap-markdown throw during parse now crashes as soon as
 * it's *opened* — even in a background tab — not just when it's the active
 * tab. React 18 unmounts the whole tree on an uncaught render/commit error
 * with no boundary above it, which is why that previously showed as the
 * entire window going blank rather than just one broken tab. `main.tsx`
 * intentionally has no top-level boundary of its own — per-document
 * containment here is more useful than an app-wide "reload" screen, since
 * the *other* open documents are perfectly fine and shouldn't be disturbed.
 *
 * Fallback is the raw Source view for just this document: `SourceEditor` is
 * a plain textarea that never runs the markdown-parse path that crashed, so
 * the document stays readable and editable even while broken in WYSIWYG.
 */
class EditorErrorBoundaryClass extends Component<Props, State> {
  state: State = { error: null, resetKey: this.props.resetKey };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    if (props.resetKey !== state.resetKey) {
      return { error: null, resetKey: props.resetKey };
    }
    return null;
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(
      `Rich editor crashed for document ${this.props.documentId}; falling back to Source view:`,
      error,
      info.componentStack
    );
  }

  render() {
    if (this.state.error) {
      return (
        <div className="h-full w-full flex flex-col">
          <div className="shrink-0 flex items-center gap-2 border-b border-amber-500/30 bg-amber-500/15 px-3 py-1.5 text-xs text-amber-700 dark:text-amber-400">
            <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
            <span>{this.props.fallbackMessage}</span>
          </div>
          <div className="flex-1 min-h-0">
            <SourceEditor documentId={this.props.documentId} active />
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

/**
 * Thin functional wrapper so the boundary's reset-on-edit behavior doesn't
 * require `EditorHost` to subscribe to document *content* (only the class
 * component above needs `resetKey`). `EditorHost` deliberately only
 * subscribes to a derived id list, not the `documents` array itself, so it
 * doesn't re-render on every keystroke (see its own doc comment) — this
 * scoped selector keeps that property: only this one document's boundary
 * re-renders when its content changes, not the whole host.
 */
export function EditorErrorBoundary({ documentId, children }: { documentId: string; children: ReactNode }) {
  const { t } = useTranslation();
  const content = useDocumentStore((state) => state.documents.find((d) => d.id === documentId)?.content ?? '');
  return (
    <EditorErrorBoundaryClass
      documentId={documentId}
      resetKey={content}
      fallbackMessage={t('editor.rich_view_failed')}
    >
      {children}
    </EditorErrorBoundaryClass>
  );
}
