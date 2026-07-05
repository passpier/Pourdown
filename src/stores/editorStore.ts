import { create } from 'zustand';
import type { Editor } from '@tiptap/react';
import type { EditorAnchor } from '@/lib/editorAnchor';

export type { EditorAnchor };

interface EditorState {
  editor: Editor | null;
  setEditor: (editor: Editor | null) => void;
  // Transient (non-persisted) anchor used to carry scroll/cursor position
  // across a WYSIWYG <-> source mode switch. Not stored in localStorage.
  pendingAnchor: EditorAnchor | null;
  setPendingAnchor: (anchor: EditorAnchor | null) => void;
  consumePendingAnchor: () => EditorAnchor | null;
}

export const useEditorStore = create<EditorState>((set, get) => ({
  editor: null,
  setEditor: (editor) => set({ editor }),
  pendingAnchor: null,
  setPendingAnchor: (anchor) => set({ pendingAnchor: anchor }),
  consumePendingAnchor: () => {
    const anchor = get().pendingAnchor;
    set({ pendingAnchor: null });
    return anchor;
  },
}));
