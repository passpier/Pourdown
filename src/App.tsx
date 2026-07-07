import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { WindowTitlebar } from 'tauri-controls';
import { Editor } from '@/components/Editor/Editor';
import { SourceEditor } from '@/components/Editor/SourceEditor';
import { Sidebar } from '@/components/Sidebar/Sidebar';
import { TabBar } from '@/components/Tabs/TabBar';
import { UnsavedCloseDialog } from '@/components/Tabs/UnsavedCloseDialog';
import { documentDisplayName } from '@/lib/documentTitle';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useEditorStore } from '@/stores/editorStore';
import { useSettingsStore } from '@/stores/settingsStore';
import { useAutoSave } from '@/hooks/useAutoSave';
import { usePlatformInitialization } from '@/hooks/usePlatformInitialization';
import { getPrimaryLanguageCode } from '@/i18n/languageUtils';
import {
  FileText,
  PanelLeft,
} from 'lucide-react';

function App() {
  const { t, i18n } = useTranslation();
  const documents = useDocumentStore((state) => state.documents);
  const activeDocumentId = useDocumentStore((state) => state.activeDocumentId);
  const closeDocument = useDocumentStore((state) => state.closeDocument);
  const createNewDocument = useDocumentStore((state) => state.createNewDocument);
  const loadDocument = useDocumentStore((state) => state.loadDocument);
  const setActiveDocument = useDocumentStore((state) => state.setActiveDocument);

  const initializeTheme = useUIStore((state) => state.initializeTheme);
  const sidebarVisible = useUIStore((state) => state.sidebarVisible);
  const toggleSidebar = useUIStore((state) => state.toggleSidebar);
  const setSidebarVisible = useUIStore((state) => state.setSidebarVisible);
  const requestSidebarSearchFocus = useUIStore((state) => state.requestSidebarSearchFocus);
  const editorMode = useUIStore((state) => state.editorMode);
  const toggleEditorMode = useUIStore((state) => state.toggleEditorMode);
  const osPlatform = useUIStore((state) => state.osPlatform);
  const setFindBarVisible = useUIStore((state) => state.setFindBarVisible);
  const language = useSettingsStore((state) => state.language);
  const hasInitializedDocument = useRef(false);
  const editor = useEditorStore((state) => state.editor);
  const menuUnlistenersRef = useRef<Array<() => void>>([]);
  const [importExportStatus, setImportExportStatus] = useState<{
    type: 'import' | 'export';
    format: string;
    state: 'loading' | 'success' | 'error';
    message?: string;
  } | null>(null);
  const [pendingClose, setPendingClose] = useState<{ id: string } | null>(null);

  // Initialize platform detection early (before first render ideally)
  usePlatformInitialization();

  // Initialize auto-save
  useAutoSave();

  // Global keyboard shortcuts for find and tab switching
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const isMac = osPlatform === 'macos';
      const modifier = isMac ? e.metaKey : e.ctrlKey;

      // Ctrl+Tab / Ctrl+Shift+Tab cycles tabs regardless of platform (this is
      // always Ctrl, never Cmd, matching browser/editor convention).
      if (e.ctrlKey && e.key === 'Tab') {
        const { documents: docs, activeDocumentId: activeId } = useDocumentStore.getState();
        if (docs.length > 1) {
          e.preventDefault();
          const currentIndex = docs.findIndex((d) => d.id === activeId);
          const delta = e.shiftKey ? -1 : 1;
          const nextIndex = (currentIndex + delta + docs.length) % docs.length;
          setActiveDocument(docs[nextIndex].id);
        }
        return;
      }

      if (!modifier) return;

      if (e.key === 'f' && !e.shiftKey) {
        e.preventDefault();
        setFindBarVisible(true);
      } else if (e.key === 'F' && e.shiftKey) {
        e.preventDefault();
        setSidebarVisible(true);
        requestSidebarSearchFocus();
      } else if (/^[1-9]$/.test(e.key)) {
        const { documents: docs } = useDocumentStore.getState();
        if (docs.length === 0) return;
        e.preventDefault();
        const index = e.key === '9' ? docs.length - 1 : Number(e.key) - 1;
        const target = docs[index];
        if (target) setActiveDocument(target.id);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [osPlatform, setFindBarVisible, setSidebarVisible, requestSidebarSearchFocus, setActiveDocument]);

  const activeDocument = documents.find(d => d.id === activeDocumentId);

  // Initialize theme on mount
  useEffect(() => {
    initializeTheme();
  }, [initializeTheme]);

  // Sync language with i18n and Rust backend
  // Ensures consistent language code format (always 'en' or 'zh', never 'en-US', 'zh-TW', etc.)
  // Persists user language preference to backend storage
  // Note: Native menu language is handled directly in Rust event handler
  useEffect(() => {
    if (language) {
      const normalizedLang = getPrimaryLanguageCode(language);
      void i18n.changeLanguage(normalizedLang);
      
      // Sync with backend state AND save preference to persistent storage
      Promise.all([
        invoke('set_language', { lang: normalizedLang }),
        invoke('save_language_preference', { lang: normalizedLang }),
      ]).catch((err) => {
        console.error('Failed to sync language to backend:', err);
      });
    }
  }, [language, i18n]);

  // Sync editor mode to native menu
  useEffect(() => {
    void invoke('update_menu_item_state', {
      id: 'view_source_code',
      checked: editorMode === 'source',
    });
  }, [editorMode]);

  // Load any pending files requested by the OS (file association) and listen for new ones.
  useEffect(() => {
    let isActive = true;
    let unlisten: (() => void) | undefined;

    const setupFileHandling = async () => {
      try {
        // 1. First set up the listener for future events (e.g. via single-instance)
        const stop = await listen<string>('open-file', (event) => {
          if (event.payload) {
            console.log('📬 Received open-file event:', event.payload);
            void loadDocument(event.payload);
          }
        });
        
        if (!isActive) {
          stop();
          return;
        }
        unlisten = stop;

        // 2. Then check for any files that arrived during startup
        const pending = await invoke<string[]>('take_pending_open_files');
        if (!isActive) return;

        if (pending.length > 0) {
          console.log('📥 Loading pending files:', pending);
          await Promise.all(
            pending.map(async (path) => {
              try {
                await loadDocument(path);
              } catch (error) {
                console.warn('Failed to load pending file:', path, error);
              }
            })
          );
        }
      } catch (error) {
        console.warn('Failed to setup file handling:', error);
      }
    };

    void setupFileHandling();

    return () => {
      isActive = false;
      unlisten?.();
    };
  }, [loadDocument]);


  const getDocumentTitle = () => {
    if (!activeDocument) return t('common.markdown_editor');

    const fileName = activeDocument.path
      ? activeDocument.path.split('/').pop() ?? t('common.untitled')
      : t('common.untitled');
    const editedSuffix = activeDocument.isDirty ? ` • ${t('common.edited')}` : '';

    return `${fileName}${editedSuffix} - ${t('common.markdown_editor')}`;
  };

  useEffect(() => {
    try {
      const currentWindow = getCurrentWindow();
      const title = getDocumentTitle();
      document.title = title;
      void currentWindow.setTitle(title);
    } catch (error) {
      console.warn('Failed to update window title:', error);
    }
  }, [activeDocument?.path, activeDocument?.isDirty]);

  const documentTitle = (() => {
    if (!activeDocument) return t('common.markdown_editor');
    return activeDocument.path
      ? activeDocument.path.split('/').pop() ?? t('common.untitled')
      : t('common.untitled');
  })();

  const charCount = activeDocument ? activeDocument.content.length : null;

  // Ensure a blank document exists for first launch
  useEffect(() => {
    if (!hasInitializedDocument.current && documents.length === 0 && !activeDocumentId) {
      createNewDocument();
      hasInitializedDocument.current = true;

    }
  }, [documents.length, activeDocumentId, createNewDocument]);

  const handleImport = useCallback(async (format: string) => {
    const extensionMap: Record<string, string[]> = {
      docx: ['docx'],
      xlsx: ['xlsx', 'xls', 'ods'],
      pdf: ['pdf'],
      pptx: ['pptx', 'ppt'],
    };
    const filterName: Record<string, string> = {
      docx: 'Word Document',
      xlsx: 'Spreadsheet',
      pdf: 'PDF Document',
      pptx: 'PowerPoint Presentation',
    };

    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: filterName[format] ?? format.toUpperCase(), extensions: extensionMap[format] ?? [format] }],
      });

      const filePath = Array.isArray(selected) ? selected[0] : selected;
      if (!filePath || typeof filePath !== 'string') return;

      setImportExportStatus({ type: 'import', format, state: 'loading' });

      const result = await invoke<{ markdown: string; media_dir: string }>(
        'import_document',
        { path: filePath, format }
      );

      const importedDoc = {
        id: crypto.randomUUID(),
        path: null,
        content: result.markdown,
        isDirty: true,
        lastSaved: null,
        assetDir: result.media_dir || null,
      };
      useDocumentStore.setState((state) => ({
        documents: [...state.documents, importedDoc],
        activeDocumentId: importedDoc.id,
      }));

      setImportExportStatus({ type: 'import', format, state: 'success' });
      setTimeout(() => setImportExportStatus(null), 3000);
    } catch (err) {
      console.error('Import failed:', err);
      setImportExportStatus({
        type: 'import',
        format,
        state: 'error',
        message: String(err),
      });
      setTimeout(() => setImportExportStatus(null), 6000);
    }
  }, []);

  const handleExport = useCallback(async (format: string) => {
    const doc = useDocumentStore.getState().documents.find(
      (d) => d.id === useDocumentStore.getState().activeDocumentId
    );
    if (!doc) return;

    const extensionMap: Record<string, string> = {
      docx: 'docx',
      xlsx: 'xlsx',
      pdf: 'pdf',
      pptx: 'pptx',
    };
    const filterName: Record<string, string> = {
      docx: 'Word Document',
      xlsx: 'Spreadsheet',
      pdf: 'PDF Document',
      pptx: 'PowerPoint Presentation',
    };

    const baseName = doc.path
      ? doc.path.split('/').pop()?.replace(/\.(md|markdown)$/, '') ?? 'document'
      : 'document';

    try {
      const filePath = await save({
        defaultPath: `${baseName}.${extensionMap[format] ?? format}`,
        filters: [{ name: filterName[format] ?? format.toUpperCase(), extensions: [extensionMap[format] ?? format] }],
      });

      if (!filePath) return;

      setImportExportStatus({ type: 'export', format, state: 'loading' });

      await invoke('export_document', { content: doc.content, path: filePath, format });

      setImportExportStatus({ type: 'export', format, state: 'success' });
      setTimeout(() => setImportExportStatus(null), 3000);
    } catch (err) {
      console.error('Export failed:', err);
      setImportExportStatus({
        type: 'export',
        format,
        state: 'error',
        message: String(err),
      });
      setTimeout(() => setImportExportStatus(null), 6000);
    }
  }, []);

  const handleOpenFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Markdown',
            extensions: ['md', 'markdown'],
          },
        ],
      });

      const filePath = Array.isArray(selected) ? selected[0] : selected;
      if (filePath && typeof filePath === 'string') {
        await loadDocument(filePath);
      }
    } catch (error) {
      console.error('Open file failed:', error);
    }
  };

  // Shared "Save As" flow, parameterized by document id so it can be driven
  // both by the active-document menu action (handleSaveAs below) and by the
  // unsaved-changes close dialog, which may target a non-active tab.
  // Returns whether the document was actually saved (false if the user
  // cancelled the native save dialog).
  const saveDocumentAs = useCallback(async (docId: string): Promise<boolean> => {
    // Flush editor content to store, bypassing the 500 ms debounce. Only the
    // active document has a live editor instance to flush from.
    if (docId === useDocumentStore.getState().activeDocumentId) {
      const editorInstance = useEditorStore.getState().editor;
      if (editorInstance) {
        const currentContent = (editorInstance.storage['markdown'] as { getMarkdown: () => string }).getMarkdown();
        useDocumentStore.getState().updateContent(docId, currentContent);
      }
    }

    try {
      const filePath = await save({
        defaultPath: 'untitled.md',
        filters: [{
          name: 'Markdown',
          extensions: ['md', 'markdown']
        }]
      });

      if (!filePath) return false;

      // Only update path; content is already correct in the store
      useDocumentStore.setState((state) => ({
        documents: state.documents.map(d =>
          d.id === docId ? { ...d, path: filePath } : d
        )
      }));
      await useDocumentStore.getState().saveDocument(docId);
      return true;
    } catch (error) {
      console.error('Save as failed:', error);
      return false;
    }
  }, []);

  const handleSaveAs = useCallback(async () => {
    const { activeDocumentId: docId } = useDocumentStore.getState();
    if (!docId) return;
    await saveDocumentAs(docId);
  }, [saveDocumentAs]);

  const handleManualSave = useCallback(async () => {
    const { activeDocumentId: docId, documents } = useDocumentStore.getState();
    if (!docId) return;
    const activeDoc = documents.find(d => d.id === docId);
    if (!activeDoc) return;

    // Flush editor content to store, bypassing the 500 ms debounce
    const editorInstance = useEditorStore.getState().editor;
    if (editorInstance) {
      const currentContent = (editorInstance.storage['markdown'] as { getMarkdown: () => string }).getMarkdown();
      useDocumentStore.getState().updateContent(docId, currentContent);
    }

    if (activeDoc.path) {
      try {
        await useDocumentStore.getState().saveDocument(docId);
      } catch (error) {
        console.error('Save failed:', error);
      }
    } else {
      await handleSaveAs();
    }
  }, [handleSaveAs]);

  // Single choke point for closing any document (tab ×, native menu, future
  // shortcuts). Flushes the live editor's content first (if this is the
  // active document) so the dirty check below is accurate, then either closes
  // immediately or opens the unsaved-changes confirm dialog.
  const requestCloseDocument = useCallback((id: string) => {
    if (id === useDocumentStore.getState().activeDocumentId) {
      const editorInstance = useEditorStore.getState().editor;
      if (editorInstance) {
        const currentContent = (editorInstance.storage['markdown'] as { getMarkdown: () => string }).getMarkdown();
        useDocumentStore.getState().updateContent(id, currentContent);
      }
    }

    const doc = useDocumentStore.getState().documents.find((d) => d.id === id);
    if (!doc || !doc.isDirty) {
      closeDocument(id);
      return;
    }
    setPendingClose({ id });
  }, [closeDocument]);

  const pendingCloseDoc = pendingClose
    ? documents.find((d) => d.id === pendingClose.id) ?? null
    : null;

  const handleConfirmSave = useCallback(async () => {
    if (!pendingClose) return;
    const { id } = pendingClose;
    const doc = useDocumentStore.getState().documents.find((d) => d.id === id);
    if (!doc) {
      setPendingClose(null);
      return;
    }

    try {
      if (doc.path) {
        await useDocumentStore.getState().saveDocument(id);
        closeDocument(id);
      } else {
        const saved = await saveDocumentAs(id);
        if (saved) closeDocument(id);
      }
    } catch (error) {
      console.error('Failed to save before closing:', error);
    } finally {
      setPendingClose(null);
    }
  }, [pendingClose, closeDocument, saveDocumentAs]);

  const handleConfirmDontSave = useCallback(() => {
    if (!pendingClose) return;
    closeDocument(pendingClose.id);
    setPendingClose(null);
  }, [pendingClose, closeDocument]);

  const handleCancelClose = useCallback(() => {
    setPendingClose(null);
  }, []);

  const runEditorCommand = (payload: { command: string; level?: number }) => {
    if (!editor) return;

    const chain = editor.chain().focus();
    switch (payload.command) {
      case 'bold':
        chain.toggleBold().run();
        break;
      case 'italic':
        chain.toggleItalic().run();
        break;
      case 'strike':
        chain.toggleStrike().run();
        break;
      case 'inline_code':
        chain.toggleCode().run();
        break;
      case 'paragraph':
        chain.setParagraph().run();
        break;
      case 'heading':
        if (payload.level) {
          chain.toggleHeading({ level: payload.level as 1 | 2 | 3 | 4 | 5 | 6 }).run();
        }
        break;
      case 'bullet_list':
        chain.toggleBulletList().run();
        break;
      case 'ordered_list':
        chain.toggleOrderedList().run();
        break;
      case 'blockquote':
        chain.toggleBlockquote().run();
        break;
      case 'code_block':
        chain.toggleCodeBlock().run();
        break;
      case 'horizontal_rule':
        chain.setHorizontalRule().run();
        break;
      case 'undo':
        editor.commands.undo();
        break;
      case 'redo':
        editor.commands.redo();
        break;
      default:
        break;
    }
  };

  // Native menu events
  useEffect(() => {
    let isActive = true;

    const setupListeners = async () => {
      try {
        const listeners = await Promise.all([
          listen('menu-new-file', () => {
            createNewDocument();
          }),
          listen('menu-open-file', () => {
            void handleOpenFile();
          }),
          listen('menu-save-file', () => {
            void handleManualSave();
          }),
          listen('menu-save-as', () => {
            void handleSaveAs();
          }),
          listen('menu-close-document', () => {
            if (activeDocumentId) {
              requestCloseDocument(activeDocumentId);
            }
          }),
          listen('menu-toggle-sidebar', () => {
            toggleSidebar();
          }),
          listen('menu-toggle-editor-mode', () => {
            toggleEditorMode();
          }),
          listen<string>('menu-set-theme', (event) => {
            const themeName = event.payload as any;
            const setCurrentTheme = useUIStore.getState().setCurrentTheme;
            setCurrentTheme(themeName);
          }),
          listen<string>('language-changed', (event) => {
            console.log('📢 Backend notified of language change:', event.payload);
            const updateSettings = useSettingsStore.getState().updateSettings;
            updateSettings({ language: event.payload });
            console.log('✅ Frontend store updated to:', event.payload);
          }),
          listen<{ command: string; level?: number }>(
            'menu-editor-command',
            (event) => {
              runEditorCommand(event.payload);
            }
          ),
          listen('menu-find', () => {
            setFindBarVisible(true);
          }),
          listen('menu-find-in-files', () => {
            setSidebarVisible(true);
            requestSidebarSearchFocus();
          }),
          listen<string>('menu-import', (e) => void handleImport(e.payload)),
          listen<string>('menu-export', (e) => void handleExport(e.payload)),
        ]);

        if (!isActive) {
          listeners.forEach((unlisten) => unlisten());
          return;
        }

        menuUnlistenersRef.current = listeners;
      } catch (error) {
        console.error('Failed to setup menu event listeners:', error);
      }
    };

    void setupListeners();

    return () => {
      isActive = false;
      menuUnlistenersRef.current.forEach(unlisten => unlisten());
      menuUnlistenersRef.current = [];
    };
  }, [editor, activeDocumentId, handleSaveAs, handleManualSave, createNewDocument, requestCloseDocument, toggleSidebar, setFindBarVisible, setSidebarVisible, requestSidebarSearchFocus, handleImport, handleExport]);

  // Enable/disable export menu items based on whether a document is active
  useEffect(() => {
    const ids = ['file_export_docx', 'file_export_xlsx', 'file_export_pdf', 'file_export_pptx'];
    ids.forEach((id) => void invoke('enable_menu_item', { id, enabled: !!activeDocument }));
  }, [activeDocument]);

  const titlebarClassName = useMemo(() => {
    if (osPlatform === 'macos') {
      return 'h-7 flex items-center border-b bg-background/95 px-3';
    }
    return 'h-10 flex items-center border-b bg-background/95 px-2';
  }, [osPlatform]);

  return (
    <div className="h-screen flex flex-col">
      <WindowTitlebar
          className={`${titlebarClassName}`}
          controlsOrder="system"
          windowControlsProps={{
            justify: true,
            platform: osPlatform ?? undefined,
            hide: osPlatform === 'macos',
          }}
        >
          <div className="flex w-full items-center gap-2">
            {osPlatform !== 'macos' && (
              <button
                type="button"
                onClick={toggleSidebar}
                className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                aria-label="Toggle sidebar"
                data-tauri-drag-region="false"
              >
                <PanelLeft className="h-3.5 w-3.5" />
              </button>
            )}
            <div
              className="flex flex-1 items-center justify-center gap-2 min-w-0"
              data-tauri-drag-region
            >
              <span className="truncate text-sm font-medium text-foreground/90">{documentTitle}</span>
              {activeDocument?.isDirty && (
                <span className="text-xs font-semibold text-amber-500">{t('common.edited')}</span>
              )}
            </div>
            {charCount !== null && (
              <span className="text-xs text-muted-foreground" data-tauri-drag-region="false">
                {t('common.char_count', { n: charCount.toLocaleString() })}
              </span>
            )}
            {osPlatform === 'macos' && (
              <button
                type="button"
                onClick={toggleSidebar}
                className="ml-auto inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                aria-label="Toggle sidebar"
                data-tauri-drag-region="false"
              >
                <PanelLeft className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
        </WindowTitlebar>
      {/* Main Content Area */}
      <div className="flex-1 flex overflow-hidden">
        {/* Sidebar */}
        <div
          className={`flex-shrink-0 transition-[width] duration-200 ${
            sidebarVisible ? 'w-72' : 'w-0 overflow-hidden pointer-events-none'
          }`}
          aria-hidden={!sidebarVisible}
        >
          <Sidebar />
        </div>

        {/* Editor Area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <TabBar requestCloseDocument={requestCloseDocument} />
          {activeDocumentId ? (
            <>
              <div className="flex-1 overflow-hidden">
                {/* Keyed by documentId so each open document gets its own Tiptap
                    instance on switch. Without this key, a single shared editor
                    instance is reused across documents, which bleeds undo/redo
                    history between tabs and lets a stale `onUpdate` closure
                    write content to the wrong document after a fast switch. */}
                {editorMode === 'wysiwyg' ? (
                  <Editor key={activeDocumentId} documentId={activeDocumentId} />
                ) : (
                  <SourceEditor key={activeDocumentId} documentId={activeDocumentId} />
                )}
              </div>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-muted-foreground">
              <div className="text-center">
                <FileText className="w-16 h-16 mx-auto mb-4 opacity-20" />
                <p className="text-lg">{t('common.no_document_open')}</p>
                <p className="text-sm mt-2">
                  {t('common.create_or_open')}
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
      {/* Import/Export status toast */}
      {importExportStatus && (
        <div className={`fixed bottom-4 right-4 z-50 rounded-lg px-4 py-3 text-sm shadow-lg max-w-sm ${
          importExportStatus.state === 'loading'
            ? 'bg-muted text-muted-foreground'
            : importExportStatus.state === 'success'
            ? 'bg-green-500/15 text-green-700 dark:text-green-400'
            : 'bg-destructive/15 text-destructive'
        }`}>
          {importExportStatus.state === 'loading' && (
            <span>
              {importExportStatus.type === 'import'
                ? t('import_export.importing')
                : t('import_export.exporting')}{' '}
              {importExportStatus.format.toUpperCase()}…
            </span>
          )}
          {importExportStatus.state === 'success' && (
            <span>
              {importExportStatus.type === 'import'
                ? t('import_export.import_success')
                : t('import_export.export_success')}
            </span>
          )}
          {importExportStatus.state === 'error' && (
            <span>{t('import_export.error_generic')}</span>
          )}
        </div>
      )}
      <UnsavedCloseDialog
        open={pendingClose !== null}
        fileName={pendingCloseDoc ? documentDisplayName(pendingCloseDoc, t('common.untitled')) : ''}
        onSave={() => void handleConfirmSave()}
        onDontSave={handleConfirmDontSave}
        onCancel={handleCancelClose}
      />
    </div>
  );
}

export default App;
