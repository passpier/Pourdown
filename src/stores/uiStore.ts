import { create } from 'zustand';
import { persist, PersistOptions } from 'zustand/middleware';
import { ThemeName, THEME_NAMES } from '@/theme/types';
import { applyTheme } from '@/theme/utils';
import { useEditorStore } from '@/stores/editorStore';

// Plain function (not a hook) — safe to call from a store action rather than
// component render. Editor instances register `captureActiveAnchor` while
// they're the visible one (see `editorStore.ts`); calling it here right
// before an editor-mode flip captures the outgoing instance's scroll
// position while its DOM is still visible/laid-out.
//
// Wrapped in try/catch and called *before* (never inside) the `set` updater
// below: with multiple documents' editor instances kept mounted (EditorHost),
// the registered capturer can belong to an instance mid-transition whose DOM
// measurement (e.g. `editor.view.nodeDOM`) throws. If that throw happened
// inside a zustand `set` updater, `set` aborts entirely and `editorMode`
// silently never flips — losing a scroll anchor must never block the mode
// toggle itself.
function captureActiveEditorAnchor(): void {
  try {
    useEditorStore.getState().captureActiveAnchor?.();
  } catch (error) {
    console.error('Failed to capture editor scroll anchor before mode switch:', error);
  }
}

interface UIState {
  currentTheme: ThemeName;
  sidebarVisible: boolean;
  fontSize: number;
  sidebarWidth: number;
  osPlatform: 'macos' | 'windows' | 'gnome' | null;
  editorMode: 'wysiwyg' | 'source';
  sidebarQuery: string;
  sidebarSearchFocusNonce: number;
  findBarVisible: boolean;
  sidebarTab: 'files' | 'outline';
  preferencesOpen: boolean;
  // Actions
  setCurrentTheme: (theme: ThemeName) => void;
  toggleTheme: () => void;
  toggleSidebar: () => void;
  setSidebarVisible: (visible: boolean) => void;
  setFontSize: (size: number) => void;
  setSidebarWidth: (width: number) => void;
  setOsPlatform: (platform: 'macos' | 'windows' | 'gnome') => void;
  setEditorMode: (mode: 'wysiwyg' | 'source') => void;
  toggleEditorMode: () => void;
  initializeTheme: () => void;
  setSidebarQuery: (query: string) => void;
  requestSidebarSearchFocus: () => void;
  setFindBarVisible: (visible: boolean) => void;
  toggleFindBar: () => void;
  setSidebarTab: (tab: 'files' | 'outline') => void;
  setPreferencesOpen: (open: boolean) => void;
  togglePreferences: () => void;
}

type PersistedUIState = Pick<UIState, 'currentTheme' | 'sidebarVisible' | 'fontSize' | 'sidebarWidth' | 'sidebarQuery' | 'sidebarTab'>;

export const useUIStore = create<UIState>()(
  persist(
    (set, get) => ({
      currentTheme: 'github-light',
      sidebarVisible: false,
      fontSize: 16,
      sidebarWidth: 280,
      editorMode: 'wysiwyg',
      sidebarQuery: '',
      sidebarSearchFocusNonce: 0,
      findBarVisible: false,
      sidebarTab: 'files',
      preferencesOpen: false,
      osPlatform: (() => {
        if (typeof navigator !== 'undefined') {
          if (navigator.userAgent.includes('Macintosh')) return 'macos';
          if (navigator.userAgent.includes('Windows')) return 'windows';
          if (navigator.userAgent.includes('Linux')) return 'gnome';
        }
        return null;
      })(),

      setCurrentTheme: (theme: ThemeName) => {
        if (THEME_NAMES[theme]) {
          applyTheme(theme);
          set({ currentTheme: theme });
        }
      },

      toggleTheme: () => {
        const currentTheme = get().currentTheme;
        const currentDefinition = THEME_NAMES[currentTheme];
        
        // Switch to dark if light, or cycle through themes
        if (currentDefinition.variant === 'light') {
          // Find first dark variant
          const darkTheme = Object.keys(THEME_NAMES).find(
            (t) => THEME_NAMES[t as ThemeName].variant === 'dark'
          ) as ThemeName | undefined;
          if (darkTheme) {
            get().setCurrentTheme(darkTheme);
            return;
          }
        } else {
          // Find first light variant
          const lightTheme = Object.keys(THEME_NAMES).find(
            (t) => THEME_NAMES[t as ThemeName].variant === 'light'
          ) as ThemeName | undefined;
          if (lightTheme) {
            get().setCurrentTheme(lightTheme);
            return;
          }
        }
      },

      toggleSidebar: () =>
        set((state) => ({
          sidebarVisible: !state.sidebarVisible,
        })),

      setSidebarVisible: (visible) => set({ sidebarVisible: visible }),

      setFontSize: (size) => set({ fontSize: size }),

      setSidebarWidth: (width) => set({ sidebarWidth: width }),

      setOsPlatform: (platform: 'macos' | 'windows' | 'gnome') =>
        set({ osPlatform: platform }),

      setEditorMode: (mode) => {
        // Capture the outgoing visible editor's scroll position *before*
        // flipping the mode, while its DOM is still visible/laid-out — see
        // `editorStore.captureActiveAnchor`'s doc comment. Editor instances
        // now stay mounted (keep-alive across tab/mode switches), so nothing
        // else triggers this capture for us.
        captureActiveEditorAnchor();
        set({ editorMode: mode });
      },

      toggleEditorMode: () => {
        // Capture *before* calling `set`, not inside its updater — see
        // `captureActiveEditorAnchor`'s doc comment above.
        captureActiveEditorAnchor();
        set((state) => ({ editorMode: state.editorMode === 'wysiwyg' ? 'source' : 'wysiwyg' }));
      },

      initializeTheme: () => {
        const state = get();
        applyTheme(state.currentTheme);
      },

      setSidebarQuery: (query) => set({ sidebarQuery: query }),

      requestSidebarSearchFocus: () =>
        set((state) => ({ sidebarSearchFocusNonce: state.sidebarSearchFocusNonce + 1 })),

      setFindBarVisible: (visible) => set({ findBarVisible: visible }),

      toggleFindBar: () =>
        set((state) => ({ findBarVisible: !state.findBarVisible })),

      setSidebarTab: (tab) => set({ sidebarTab: tab }),

      setPreferencesOpen: (open) => set({ preferencesOpen: open }),

      togglePreferences: () =>
        set((state) => ({ preferencesOpen: !state.preferencesOpen })),
    }),
    {
      name: 'ui-preferences',
      version: 5,
      // v3 -> v4: the theme set was overhauled (github-dark, nord-light,
      // solarized-light dropped; nord-dark renamed to nord). v4 -> v5:
      // solarized-dark replaced by solarized-light (rebalancing the
      // light/dark theme split). Remap anyone persisted on a removed key to
      // its closest replacement so applyTheme doesn't silently no-op and
      // strand them on stale CSS defaults. Also drops the removed
      // `fontFamily` field (now theme-driven), which `partialize` simply
      // omits going forward.
      migrate: (persistedState) => {
        const state = persistedState as Partial<UIState> | undefined;
        if (!state || typeof state !== 'object') return state;

        const themeMigrations: Record<string, ThemeName> = {
          'github-dark': 'one-dark-pro',
          'nord-dark': 'nord',
          'nord-light': 'nord',
          'solarized-dark': 'solarized-light',
        };
        const currentTheme = state.currentTheme as unknown as string | undefined;
        if (currentTheme && currentTheme in themeMigrations) {
          return { ...state, currentTheme: themeMigrations[currentTheme] };
        }
        return state;
      },
      partialize: (state): PersistedUIState => ({
        currentTheme: state.currentTheme,
        sidebarVisible: state.sidebarVisible,
        fontSize: state.fontSize,
        sidebarWidth: state.sidebarWidth,
        sidebarQuery: state.sidebarQuery,
        sidebarTab: state.sidebarTab,
        // osPlatform, editorMode, and findBarVisible are excluded from persistence
      }),
      onRehydrate: (state: unknown) => {
        // Apply theme after hydration from localStorage
        if (state && typeof state === 'object' && 'currentTheme' in state) {
          const uiState = state as Partial<UIState>;
          if (uiState.currentTheme) {
            applyTheme(uiState.currentTheme);
          }
        }
      },
    } as PersistOptions<UIState, PersistedUIState>
  )
);
