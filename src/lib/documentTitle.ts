/**
 * Shared display-name logic for a document, used by the title bar and the tab
 * bar so the two stay consistent. A document with no `path` (never saved) is
 * shown using the caller-supplied "untitled" label (already translated by the
 * caller via i18next).
 */
export function documentDisplayName(
  doc: { path: string | null },
  untitledLabel: string
): string {
  if (!doc.path) return untitledLabel;
  return doc.path.split('/').pop() ?? untitledLabel;
}
