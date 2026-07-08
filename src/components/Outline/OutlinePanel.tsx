import { useEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { scanMarkdownHeadings } from '@/lib/editorAnchor';
import { useEditorStore } from '@/stores/editorStore';

interface OutlinePanelProps {
  content: string;
}

export function OutlinePanel({ content }: OutlinePanelProps) {
  const { t } = useTranslation();
  const headings = useMemo(() => scanMarkdownHeadings(content), [content]);
  const activeHeadingIndex = useEditorStore((state) => state.activeHeadingIndex);
  const requestScrollToHeading = useEditorStore((state) => state.requestScrollToHeading);
  const activeRowRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    activeRowRef.current?.scrollIntoView({ block: 'nearest' });
  }, [activeHeadingIndex]);

  if (headings.length === 0) {
    return (
      <div className="px-2 py-3 text-xs text-muted-foreground">
        {t('sidebar.no_headings')}
      </div>
    );
  }

  return (
    <div className="space-y-1 px-1 pb-1">
      {headings.map((heading) => {
        const isActive = heading.index === activeHeadingIndex;
        return (
          <button
            key={heading.index}
            ref={isActive ? activeRowRef : undefined}
            type="button"
            onClick={() => requestScrollToHeading(heading.index)}
            className={cn('sidebar-row w-full text-left', isActive && 'sidebar-row-active')}
            style={{ paddingLeft: `${0.5 + (heading.level - 1) * 0.75}rem` }}
            title={heading.text}
          >
            <span className="truncate text-sm">{heading.text || ' '}</span>
          </button>
        );
      })}
    </div>
  );
}
