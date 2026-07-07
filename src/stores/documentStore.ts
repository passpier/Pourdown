import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface Document {
  id: string;
  path: string | null;
  content: string;
  isDirty: boolean;
  lastSaved: Date | null;
  /**
   * Staging directory (under the app's `imports/` dir) holding sidecar images
   * extracted during import, e.g. `<assetDir>/assets/image1.png`. Non-null
   * only for imported documents that haven't yet been relocated next to a
   * saved `.md` file (see `saveDocument`). `null`/undefined for documents
   * with no extracted images, or once relocation has happened.
   */
  assetDir?: string | null;
}

/** Splits a path on both `/` and `\` so this works for Windows paths too. */
function splitPath(path: string): string[] {
  return path.split(/[/\\]/);
}

function dirname(path: string): string {
  const parts = splitPath(path);
  parts.pop();
  return parts.join('/');
}

function basenameWithoutExt(path: string): string {
  const name = splitPath(path).pop() ?? 'document';
  return name.replace(/\.(md|markdown)$/i, '') || 'document';
}

interface DocumentState {
  documents: Document[];
  activeDocumentId: string | null;
  // Actions
  openDocument: (doc: Document) => void;
  closeDocument: (id: string) => void;
  setActiveDocument: (id: string) => void;
  updateContent: (id: string, content: string) => void;
  saveDocument: (id: string) => Promise<void>;
  loadDocument: (path: string) => Promise<void>;
  createNewDocument: () => void;
  reorderDocuments: (fromIndex: number, toIndex: number) => void;
}

export const useDocumentStore = create<DocumentState>((set, get) => ({
  documents: [],
  activeDocumentId: null,

  openDocument: (doc) => {
    const { documents } = get();
    // Check if document is already open
    const existingDoc = documents.find(d => d.path === doc.path);
    if (existingDoc) {
      set({ activeDocumentId: existingDoc.id });
      return;
    }
    
    set((state) => ({
      documents: [...state.documents, doc],
      activeDocumentId: doc.id,
    }));
  },

  closeDocument: (id) =>
    set((state) => {
      // A document closed while it still has an `assetDir` was never saved
      // (relocation clears it), so its staging media dir is now orphaned —
      // discard it rather than waiting for the next app-start prune.
      const closing = state.documents.find(d => d.id === id);
      if (closing?.assetDir) {
        void invoke('discard_media', { importDir: closing.assetDir }).catch((error) => {
          console.error('Failed to discard staged import media:', error);
        });
      }

      const remainingDocs = state.documents.filter(d => d.id !== id);
      let newActiveId = state.activeDocumentId;
      
      if (state.activeDocumentId === id) {
        // If closing active document, switch to another one
        if (remainingDocs.length > 0) {
          const closedIndex = state.documents.findIndex(d => d.id === id);
          // Try to activate the next document, or previous if it was the last
          newActiveId = remainingDocs[Math.min(closedIndex, remainingDocs.length - 1)]?.id || null;
        } else {
          newActiveId = null;
        }
      }
      
      return {
        documents: remainingDocs,
        activeDocumentId: newActiveId,
      };
    }),

  setActiveDocument: (id) => set({ activeDocumentId: id }),

  updateContent: (id, content) =>
    set((state) => ({
      documents: state.documents.map(d =>
        d.id === id ? { ...d, content, isDirty: true } : d
      ),
    })),

  saveDocument: async (id) => {
    const doc = get().documents.find(d => d.id === id);
    if (!doc?.path) {
      throw new Error('Document has no path');
    }

    try {
      let content = doc.content;
      let assetDir = doc.assetDir ?? null;

      // First save of an imported document with sidecar images: move them
      // from the temporary `imports/` staging dir to `<name>.assets/` next to
      // the saved `.md`, and rewrite the markdown's `assets/...` links to
      // point at the new relative folder so the pair stays portable.
      if (assetDir) {
        const sidecarDirName = `${basenameWithoutExt(doc.path)}.assets`;
        const targetDir = `${dirname(doc.path)}/${sidecarDirName}`;
        await invoke('relocate_media', { from: `${assetDir}/assets`, to: targetDir });
        content = content.split('](assets/').join(`](${sidecarDirName}/`);
        assetDir = null;
      }

      await invoke('save_markdown_file', {
        path: doc.path,
        content,
      });

      // Add to recent files
      await invoke('add_recent_file', { path: doc.path });

      set((state) => ({
        documents: state.documents.map(d =>
          d.id === id
            ? { ...d, content, assetDir, isDirty: false, lastSaved: new Date() }
            : d
        ),
      }));
    } catch (error) {
      console.error('Save failed:', error);
      throw error;
    }
  },

  loadDocument: async (path) => {
    try {
      const content = await invoke<string>('read_markdown_file', { path });
      
      const doc: Document = {
        id: crypto.randomUUID(),
        path,
        content,
        isDirty: false,
        lastSaved: new Date(),
      };
      
      // Add to recent files
      await invoke('add_recent_file', { path });
      
      get().openDocument(doc);
    } catch (error) {
      console.error('Load failed:', error);
      throw error;
    }
  },

  createNewDocument: () => {
    const doc: Document = {
      id: crypto.randomUUID(),
      path: null,
      content: '',
      isDirty: false,
      lastSaved: null,
    };
    
    set((state) => ({
      documents: [...state.documents, doc],
      activeDocumentId: doc.id,
    }));
  },

  reorderDocuments: (fromIndex, toIndex) =>
    set((state) => {
      if (
        fromIndex === toIndex ||
        fromIndex < 0 ||
        toIndex < 0 ||
        fromIndex >= state.documents.length ||
        toIndex >= state.documents.length
      ) {
        return state;
      }
      const documents = [...state.documents];
      const [moved] = documents.splice(fromIndex, 1);
      documents.splice(toIndex, 0, moved);
      return { documents };
    }),
}));
