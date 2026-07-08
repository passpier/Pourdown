import { create } from 'zustand';
import type { Editor } from '@tiptap/react';
import type { EditorAnchor } from '@/lib/editorAnchor';

export type { EditorAnchor };

interface ScrollToHeadingRequest {
  index: number;
  nonce: number;
}

interface EditorState {
  editor: Editor | null;
  setEditor: (editor: Editor | null) => void;
  // Transient (non-persisted) anchor used to carry scroll/cursor position
  // across a WYSIWYG <-> source mode switch. Not stored in localStorage.
  pendingAnchor: EditorAnchor | null;
  setPendingAnchor: (anchor: EditorAnchor | null) => void;
  consumePendingAnchor: () => EditorAnchor | null;
  // Outline scroll-spy: the heading currently at the top of the viewport,
  // reported by whichever editor (WYSIWYG or source) is mounted. Consumed by
  // OutlinePanel to highlight the active row.
  activeHeadingIndex: number | null;
  setActiveHeadingIndex: (index: number | null) => void;
  // Outline click-to-scroll: incrementing the nonce re-fires the request even
  // if the same heading is clicked twice in a row.
  scrollToHeadingRequest: ScrollToHeadingRequest | null;
  requestScrollToHeading: (index: number) => void;
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
  activeHeadingIndex: null,
  setActiveHeadingIndex: (index) => set({ activeHeadingIndex: index }),
  scrollToHeadingRequest: null,
  requestScrollToHeading: (index) =>
    set((state) => ({
      scrollToHeadingRequest: {
        index,
        nonce: (state.scrollToHeadingRequest?.nonce ?? 0) + 1,
      },
    })),
}));
