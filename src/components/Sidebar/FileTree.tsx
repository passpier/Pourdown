import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronDown, ChevronRight, File, Folder, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';

interface FileEntry {
  name: string;
  path: string;
  is_directory: boolean;
}

// Only markdown files (and directories, to keep browsing them) are shown —
// matches the filter the Sidebar previously applied to its one-level listing.
const filterEntries = (entries: FileEntry[]) =>
  entries.filter(
    (entry) =>
      entry.is_directory ||
      entry.name.endsWith('.md') ||
      entry.name.endsWith('.markdown'),
  );

interface TreeNodeProps {
  entry: FileEntry;
  depth: number;
  activePath: string | null;
  onOpenFile: (entry: FileEntry) => void;
  defaultExpanded?: boolean;
}

function TreeNode({ entry, depth, activePath, onOpenFile, defaultExpanded }: TreeNodeProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(!!defaultExpanded);
  const [children, setChildren] = useState<FileEntry[] | null>(null);
  // Derived, not stored: we're loading whenever this folder is expanded but
  // its children haven't arrived yet. Avoids a setState call at the top of
  // the effect body (which would trigger an extra render pass).
  const loading = expanded && children === null;

  // Lazily fetch this folder's children the first time it's expanded (or
  // immediately for a root node mounted with defaultExpanded).
  useEffect(() => {
    if (!entry.is_directory || !expanded || children !== null) return;
    let cancelled = false;
    invoke<FileEntry[]>('list_directory', { path: entry.path })
      .then((result) => {
        if (!cancelled) setChildren(filterEntries(result));
      })
      .catch((error) => {
        console.error('Failed to list directory:', error);
        if (!cancelled) setChildren([]);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [expanded, entry.path]);

  const indent = 8 + depth * 15;

  if (!entry.is_directory) {
    const isActive = entry.path === activePath;
    return (
      <button
        type="button"
        onClick={() => onOpenFile(entry)}
        title={entry.path}
        className={cn('sidebar-row w-full text-left', isActive && 'sidebar-row-active')}
        style={{ paddingLeft: `${indent}px` }}
      >
        <File className="h-3.5 w-3.5 flex-shrink-0 text-muted-foreground" />
        <span className="truncate">{entry.name}</span>
      </button>
    );
  }

  return (
    <div>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        title={entry.path}
        className="sidebar-row w-full text-left"
        style={{ paddingLeft: `${indent}px` }}
      >
        {expanded ? (
          <ChevronDown className="h-3.5 w-3.5 flex-shrink-0" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 flex-shrink-0" />
        )}
        <Folder className="h-3.5 w-3.5 flex-shrink-0 text-sky-600" />
        <span className="truncate">{entry.name}</span>
      </button>

      {expanded && (
        <div>
          {loading && (
            <div
              className="flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground"
              style={{ paddingLeft: `${indent + 15}px` }}
            >
              <Loader2 className="h-3 w-3 animate-spin" />
              {t('common.loading')}
            </div>
          )}
          {!loading && children !== null && children.length === 0 && (
            <div
              className="px-2 py-1.5 text-xs text-muted-foreground"
              style={{ paddingLeft: `${indent + 15}px` }}
            >
              {t('sidebar.no_files_found')}
            </div>
          )}
          {!loading &&
            children?.map((child) => (
              <TreeNode
                key={child.path}
                entry={child}
                depth={depth + 1}
                activePath={activePath}
                onOpenFile={onOpenFile}
              />
            ))}
        </div>
      )}
    </div>
  );
}

interface FileTreeProps {
  rootDir: string;
  activePath: string | null;
  onOpenFile: (entry: FileEntry) => void;
}

export function FileTree({ rootDir, activePath, onOpenFile }: FileTreeProps) {
  const rootEntry: FileEntry = {
    name: rootDir.split('/').pop() ?? rootDir,
    path: rootDir,
    is_directory: true,
  };

  return (
    <div className="space-y-0.5 px-1 pb-1">
      <TreeNode
        // Re-mount (and re-fetch) the tree whenever the opened folder changes.
        key={rootDir}
        entry={rootEntry}
        depth={0}
        activePath={activePath}
        onOpenFile={onOpenFile}
        defaultExpanded
      />
    </div>
  );
}
