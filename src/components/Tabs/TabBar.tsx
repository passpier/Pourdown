import { memo, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, X } from 'lucide-react';
import { useDocumentStore } from '@/stores/documentStore';
import { documentDisplayName } from '@/lib/documentTitle';
import { cn } from '@/lib/utils';

interface TabBarProps {
  requestCloseDocument: (id: string) => void;
}

export const TabBar = memo(function TabBar({ requestCloseDocument }: TabBarProps) {
  const { t } = useTranslation();
  const documents = useDocumentStore((state) => state.documents);
  const activeDocumentId = useDocumentStore((state) => state.activeDocumentId);
  const setActiveDocument = useDocumentStore((state) => state.setActiveDocument);
  const createNewDocument = useDocumentStore((state) => state.createNewDocument);
  const reorderDocuments = useDocumentStore((state) => state.reorderDocuments);

  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  const activeTabRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    activeTabRef.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [activeDocumentId]);

  if (documents.length === 0) return null;

  const untitledLabel = t('common.untitled');

  const handleDrop = (targetIndex: number) => {
    if (dragIndex !== null && dragIndex !== targetIndex) {
      reorderDocuments(dragIndex, targetIndex);
    }
    setDragIndex(null);
    setDragOverIndex(null);
  };

  return (
    <div
      className="flex h-9 flex-shrink-0 items-stretch overflow-x-auto border-b bg-background/95"
      role="tablist"
    >
      {documents.map((doc, index) => {
        const isActive = doc.id === activeDocumentId;
        const name = documentDisplayName(doc, untitledLabel);
        return (
          <button
            key={doc.id}
            ref={isActive ? activeTabRef : undefined}
            type="button"
            role="tab"
            aria-selected={isActive}
            draggable
            onDragStart={() => setDragIndex(index)}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOverIndex(index);
            }}
            onDragLeave={() => setDragOverIndex((v) => (v === index ? null : v))}
            onDrop={(e) => {
              e.preventDefault();
              handleDrop(index);
            }}
            onDragEnd={() => {
              setDragIndex(null);
              setDragOverIndex(null);
            }}
            onClick={() => setActiveDocument(doc.id)}
            title={doc.path ?? untitledLabel}
            className={cn(
              'group flex flex-shrink-0 max-w-[180px] items-center gap-1.5 border-r px-3 text-xs transition-colors',
              isActive
                ? 'bg-accent text-foreground'
                : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
              dragOverIndex === index && dragIndex !== null && dragIndex !== index
                ? 'border-l-2 border-l-primary'
                : ''
            )}
          >
            {doc.isDirty && (
              <span className="h-1.5 w-1.5 flex-shrink-0 rounded-full bg-amber-500" aria-hidden />
            )}
            <span className="truncate">{name}</span>
            <span
              role="button"
              tabIndex={-1}
              onClick={(e) => {
                e.stopPropagation();
                requestCloseDocument(doc.id);
              }}
              title={t('tabs.close_tab')}
              className="ml-1 flex h-4 w-4 flex-shrink-0 items-center justify-center rounded-sm opacity-0 hover:bg-muted-foreground/20 group-hover:opacity-100"
            >
              <X className="h-3 w-3" />
            </span>
          </button>
        );
      })}
      <button
        type="button"
        onClick={() => createNewDocument()}
        title={t('tabs.new_tab')}
        className="flex w-9 flex-shrink-0 items-center justify-center text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
      >
        <Plus className="h-3.5 w-3.5" />
      </button>
    </div>
  );
});
