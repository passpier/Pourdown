import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface Document {
  id: string;
  path: string | null;
  content: string;
  isDirty: boolean;
  lastSaved: Date | null;
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
      await invoke('save_markdown_file', {
        path: doc.path,
        content: doc.content,
      });
      
      // Add to recent files
      await invoke('add_recent_file', { path: doc.path });
      
      set((state) => ({
        documents: state.documents.map(d =>
          d.id === id
            ? { ...d, isDirty: false, lastSaved: new Date() }
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
