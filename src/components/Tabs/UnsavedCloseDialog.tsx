import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';

interface UnsavedCloseDialogProps {
  open: boolean;
  fileName: string;
  onSave: () => void;
  onDontSave: () => void;
  onCancel: () => void;
}

/**
 * Lightweight centered confirm modal for closing a document with unsaved
 * changes. No generic Dialog primitive exists yet in `components/ui`, so this
 * is a small self-contained overlay built from the existing Button component
 * and the app's Tailwind design tokens.
 */
export function UnsavedCloseDialog({
  open,
  fileName,
  onSave,
  onDontSave,
  onCancel,
}: UnsavedCloseDialogProps) {
  const { t } = useTranslation();

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        onSave();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [open, onCancel, onSave]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40"
      role="dialog"
      aria-modal="true"
      aria-labelledby="unsaved-close-dialog-title"
    >
      <div className="w-full max-w-sm rounded-lg border bg-background p-5 shadow-lg">
        <h2 id="unsaved-close-dialog-title" className="text-sm font-semibold text-foreground">
          {t('dialog.unsaved_title')}
        </h2>
        <p className="mt-2 text-sm text-muted-foreground">
          {t('dialog.unsaved_message', { name: fileName })}
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t('dialog.cancel')}
          </Button>
          <Button variant="outline" size="sm" onClick={onDontSave}>
            {t('dialog.dont_save')}
          </Button>
          <Button variant="default" size="sm" onClick={onSave}>
            {t('dialog.save')}
          </Button>
        </div>
      </div>
    </div>
  );
}
