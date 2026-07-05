import { create } from 'zustand';
import type { Editor } from '@tiptap/react';

export interface EditorAnchor {
  documentId: string;
  /** Ordinal of the nearest heading at/above the cursor; -1 if none */
  headingIndex: number;
  /** Heading text, used to validate the anchor still makes sense on restore */
  headingText: string;
  /** Fallback: fraction (0-1) of the way through the scrollable content */
  scrollRatio: number;
}

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
