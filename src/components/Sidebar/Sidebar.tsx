import { memo, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import {
  ChevronDown,
  ChevronRight,
  FilePlus,
  FolderOpen,
  Home,
  Search,
} from 'lucide-react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useDocumentStore } from '@/stores/documentStore';
import { useUIStore } from '@/stores/uiStore';
import { useRecentFiles } from '@/hooks/useRecentFiles';
import { FileItem } from './FileItem';
import { SearchPanel } from '@/components/Search/SearchPanel';
import { OutlinePanel } from '@/components/Outline/OutlinePanel';

interface FileEntry {
  name: string;
  path: string;
  is_directory: boolean;
}

export const Sidebar = memo(function Sidebar() {
  const { t } = useTranslation();
  const [currentDirectory, setCurrentDirectory] = useState<string | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [filesCollapsed, setFilesCollapsed] = useState(false);
  const { recentFiles, refresh: refreshRecent } = useRecentFiles();

  const loadDocument = useDocumentStore((state) => state.loadDocument);
  const createNewDocument = useDocumentStore((state) => state.createNewDocument);
  const activeDocumentId = useDocumentStore((state) => state.activeDocumentId);
  const documents = useDocumentStore((state) => state.documents);

  const sidebarQuery = useUIStore((state) => state.sidebarQuery);
  const setSidebarQuery = useUIStore((state) => state.setSidebarQuery);
  const sidebarSearchFocusNonce = useUIStore((state) => state.sidebarSearchFocusNonce);
  const sidebarTab = useUIStore((state) => state.sidebarTab);
  const setSidebarTab = useUIStore((state) => state.setSidebarTab);

  const searchInputRef = useRef<HTMLInputElement>(null);
  const activeDocument = documents.find((d) => d.id === activeDocumentId);
  const hasSearchQuery = sidebarQuery.trim().length > 0;

  useEffect(() => {
    if (sidebarSearchFocusNonce > 0) {
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    }
  }, [sidebarSearchFocusNonce]);

  // A search query's results render under the Files tab; jump there
  // automatically so a query typed while on the Outline tab isn't invisible.
  useEffect(() => {
    if (hasSearchQuery) {
      setSidebarTab('files');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasSearchQuery]);

  const currentWorkspaceName = useMemo(() => {
    if (!currentDirectory) return t('sidebar.workspace_empty');
    return currentDirectory.split('/').pop() ?? currentDirectory;
  }, [currentDirectory, t]);

  const loadDirectory = async (path: string) => {
    setLoading(true);
    try {
      const entries = await invoke<FileEntry[]>('list_directory', { path });
      const filtered = entries.filter(
        (entry) =>
          entry.is_directory ||
          entry.name.endsWith('.md') ||
          entry.name.endsWith('.markdown'),
      );
      setFiles(filtered);
      setCurrentDirectory(path);
    } catch (error) {
      console.error('Failed to load directory:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleOpenFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });

      if (selected && typeof selected === 'string') {
        await loadDirectory(selected);
      }
    } catch (error) {
      console.error('Failed to open folder:', error);
    }
  };

  const handleFileClick = async (file: FileEntry) => {
    if (file.is_directory) {
      await loadDirectory(file.path);
      return;
    }

    if (file.name.endsWith('.md') || file.name.endsWith('.markdown')) {
      try {
        await loadDocument(file.path);
        refreshRecent();
      } catch (error) {
        console.error('Failed to load document:', error);
      }
    }
  };

  const handleRecentFileClick = async (path: string) => {
    try {
      await loadDocument(path);
      refreshRecent();
    } catch (error) {
      console.error('Failed to load recent file:', error);
    }
  };

  const handleNewFile = () => {
    createNewDocument();
  };

  const goToParentDirectory = () => {
    if (!currentDirectory) return;
    const parentPath = currentDirectory.split('/').slice(0, -1).join('/');
    if (parentPath) {
      void loadDirectory(parentPath);
    }
  };

  return (
    <div className="sidebar-shell">
      <section className="sidebar-card">
        <div className="sidebar-card-header">
          <button
            type="button"
            onClick={goToParentDirectory}
            disabled={!currentDirectory}
            className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-foreground/80 transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
            title={currentDirectory ?? t('sidebar.workspace_empty')}
          >
            <span className="max-w-[9.5rem] truncate">{currentWorkspaceName}</span>
            <ChevronDown className="h-3 w-3" />
          </button>
          <span className="text-[10px] text-muted-foreground">⌘⇧F</span>
        </div>

        <div className="sidebar-card-content space-y-2">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <input
              ref={searchInputRef}
              type="text"
              value={sidebarQuery}
              onChange={(event) => setSidebarQuery(event.target.value)}
              placeholder={t('search.placeholder')}
              className="h-8 w-full rounded-md border border-input bg-background pl-7 pr-2 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleOpenFolder}
              className="h-8 flex-1 rounded-lg border-[hsl(var(--sidebar-border))] bg-[hsl(var(--sidebar-surface))] text-xs hover:bg-[hsl(var(--sidebar-surface-strong))]"
            >
              <FolderOpen className="mr-1.5 h-3.5 w-3.5" />
              {t('sidebar.open_folder')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handleNewFile}
              title={t('sidebar.new_file')}
              className="h-8 rounded-lg border-[hsl(var(--sidebar-border))] bg-[hsl(var(--sidebar-surface))] px-2 hover:bg-[hsl(var(--sidebar-surface-strong))]"
            >
              <FilePlus className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      </section>

      <div className="mt-3 flex gap-1 rounded-lg bg-[hsl(var(--sidebar-surface))] p-1">
        <button
          type="button"
          onClick={() => setSidebarTab('files')}
          className={cn(
            'flex-1 rounded-md px-2 py-1 text-xs font-medium transition-colors',
            sidebarTab === 'files'
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          )}
        >
          {t('sidebar.tab_files')}
        </button>
        <button
          type="button"
          onClick={() => setSidebarTab('outline')}
          className={cn(
            'flex-1 rounded-md px-2 py-1 text-xs font-medium transition-colors',
            sidebarTab === 'outline'
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'
          )}
        >
          {t('sidebar.tab_outline')}
        </button>
      </div>

      <ScrollArea className="mt-3 flex-1">
        {sidebarTab === 'files' ? (
          <>
            {recentFiles.length > 0 && (
              <section className="sidebar-card">
                <div className="sidebar-card-header">
                  <span className="sidebar-section-title">{t('sidebar.recent_files')}</span>
                </div>
                <div className="sidebar-card-content space-y-1">
                  {recentFiles.slice(0, 5).map((path) => (
                    <button
                      key={path}
                      type="button"
                      onClick={() => handleRecentFileClick(path)}
                      className={`sidebar-row w-full text-left ${
                        activeDocument?.path === path ? 'sidebar-row-active' : ''
                      }`}
                      title={path}
                    >
                      <span className="truncate">{path.split('/').pop()}</span>
                    </button>
                  ))}
                </div>
              </section>
            )}

            <section className="sidebar-card mt-3">
              <div className="sidebar-card-header">
                <button
                  type="button"
                  onClick={() => setFilesCollapsed((v) => !v)}
                  className="inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-xs font-semibold text-foreground/80 transition-colors hover:bg-accent"
                >
                  {filesCollapsed ? (
                    <ChevronRight className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronDown className="h-3.5 w-3.5" />
                  )}
                  {t('sidebar.files')}
                </button>
                {currentDirectory && (
                  <button
                    type="button"
                    onClick={goToParentDirectory}
                    className="sidebar-icon-button"
                    title={t('sidebar.parent_directory')}
                  >
                    <Home className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>

              {!filesCollapsed && (
                <div className="sidebar-card-content">
                  {loading ? (
                    <div className="px-2 py-3 text-xs text-muted-foreground">{t('common.loading')}</div>
                  ) : currentDirectory ? (
                    files.length > 0 ? (
                      <div className="space-y-1 px-1 pb-1">
                        {files.map((file) => (
                          <FileItem
                            key={file.path}
                            file={file}
                            onClick={() => void handleFileClick(file)}
                            isActive={activeDocument?.path === file.path}
                          />
                        ))}
                      </div>
                    ) : (
                      <div className="px-2 py-3 text-xs text-muted-foreground">
                        {t('sidebar.no_files_found')}
                      </div>
                    )
                  ) : (
                    <div className="px-2 py-3 text-xs text-muted-foreground whitespace-nowrap">
                      {t('sidebar.open_folder_to_browse')}
                    </div>
                  )}
                </div>
              )}
            </section>

            {hasSearchQuery && (
              <SearchPanel
                currentDirectory={currentDirectory}
                query={sidebarQuery}
              />
            )}
          </>
        ) : (
          <OutlinePanel content={activeDocument?.content ?? ''} />
        )}
      </ScrollArea>
    </div>
  );
});
