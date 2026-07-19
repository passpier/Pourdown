import { memo, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FileText } from 'lucide-react';
import { Editor } from './Editor';
import { SourceEditor } from './SourceEditor';
import { EditorErrorBoundary } from './EditorErrorBoundary';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';

/**
 * Max number of documents whose editor instances are kept mounted (alive but
 * hidden) at once. Bounds memory for users who open many tabs; revisiting a
 * document evicted past this cap just re-mounts it (one re-parse — the same
 * cost every switch used to pay before keep-alive).
 */
const KEEP_ALIVE_MAX = 8;

/**
 * Renders one `<Editor>` + `<SourceEditor>` pair per recently-visited
 * document and keeps them all mounted — toggling `hidden`/`active` instead of
 * unmounting — so tab switches and preview<->source toggles are instant (no
 * Tiptap re-parse, no remount).
 *
 * Previously `App.tsx` rendered a single instance keyed by
 * `activeDocumentId` (`key={activeDocumentId}`), which forced React to fully
 * unmount the outgoing document's editor and mount a fresh one for the
 * incoming document on *every* tab switch — re-parsing the whole document
 * into ProseMirror and re-mounting a React node view for every code/math/HTML
 * block. Keeping instances alive removes that cost for every switch after the
 * first visit to a tab.
 *
 * Each pair is wrapped in its own `EditorErrorBoundary` (see that file's doc
 * comment): keeping every visited document's editor mounted means a document
 * whose content crashes Tiptap's parse now does so as soon as it's *opened*,
 * even in a background tab, not just when active — the boundary contains
 * that to the one document's pane instead of blanking the whole app.
 */
export const EditorHost = memo(function EditorHost() {
  const { t } = useTranslation();
  const activeDocumentId = useDocumentStore((state) => state.activeDocumentId);
  const editorMode = useUIStore((state) => state.editorMode);
  // A primitive derived from the documents array (not the array itself), so
  // this only changes when a tab actually opens/closes/reorders — not on
  // every keystroke (which recreates the `documents` array via
  // `updateContent`). That keeps the LRU-maintenance effect below from
  // re-running on every edit.
  const documentIdsKey = useDocumentStore((state) => state.documents.map((d) => d.id).join('|'));

  // LRU of visited document ids, most-recent last. Each kept id gets exactly
  // one <Editor> and one <SourceEditor> mounted (both, regardless of the
  // current mode) so both tab switches and preview<->source toggles are
  // instant; each one self-hides via its own `active` prop when it isn't the
  // visible combination of (active tab, active mode).
  const [keptIds, setKeptIds] = useState<string[]>([]);

  useEffect(() => {
    const liveIds = new Set(documentIdsKey ? documentIdsKey.split('|') : []);
    setKeptIds((prev) => {
      // Drop ids for tabs that have since been closed, so a stale instance
      // doesn't linger (e.g. holding an orphaned assetDir reference).
      let next = prev.filter((id) => liveIds.has(id));
      if (activeDocumentId && liveIds.has(activeDocumentId)) {
        // Move the active id to the most-recently-used end.
        next = next.filter((id) => id !== activeDocumentId);
        next.push(activeDocumentId);
      }
      // Evict the least-recently-used ids past the cap. The just-activated
      // id was pushed to the end above, so it's never the one evicted here.
      if (next.length > KEEP_ALIVE_MAX) {
        next = next.slice(next.length - KEEP_ALIVE_MAX);
      }
      return next;
    });
  }, [documentIdsKey, activeDocumentId]);

  // `keptIds` is only updated by the effect above, which runs *after* the
  // render where `activeDocumentId` changes to a document not yet in the
  // list (e.g. every document opened via the `Promise.all` multi-file path
  // in App.tsx). Rendering raw `keptIds` on that render would give every
  // pane below `hidden={id !== activeDocumentId}` — none of them the active
  // id — i.e. a one-frame fully blank editor area, repeated once per file in
  // a multi-file open. Unioning in `activeDocumentId` here (render time, not
  // state) guarantees the active document always has a rendered pane even
  // before the LRU effect catches up on the next commit.
  const renderIds = useMemo(() => {
    if (!activeDocumentId || keptIds.includes(activeDocumentId)) return keptIds;
    return [...keptIds, activeDocumentId];
  }, [keptIds, activeDocumentId]);

  if (!activeDocumentId) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted-foreground">
        <div className="text-center">
          <FileText className="w-16 h-16 mx-auto mb-4 opacity-20" />
          <p className="text-lg">{t('common.no_document_open')}</p>
          <p className="text-sm mt-2">{t('common.create_or_open')}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-hidden">
      {renderIds.map((id) => (
        <div key={id} className="h-full w-full" hidden={id !== activeDocumentId}>
          {/* Both siblings share one boundary (rather than wrapping just
              `<Editor>`) so a crash swaps them both out for the fallback's
              single forced-active `SourceEditor` — wrapping only `<Editor>`
              would leave this real `<SourceEditor>` free to also go active
              if the user later toggles the global mode to 'source', stacking
              two live textareas for the same document in one pane. */}
          <EditorErrorBoundary documentId={id}>
            <Editor documentId={id} active={id === activeDocumentId && editorMode === 'wysiwyg'} />
            <SourceEditor documentId={id} active={id === activeDocumentId && editorMode === 'source'} />
          </EditorErrorBoundary>
        </div>
      ))}
    </div>
  );
});
