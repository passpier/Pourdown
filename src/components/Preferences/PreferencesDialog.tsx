import { useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Palette, Settings as SettingsIcon, Type, X } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { Select } from '@/components/ui/select';
import { cn } from '@/lib/utils';
import { useUIStore } from '@/stores/uiStore';
import { useSettingsStore } from '@/stores/settingsStore';
import { ALL_THEMES } from '@/theme/types';
import type { ThemeName } from '@/theme/types';

type Category = 'general' | 'appearance' | 'editor';

interface CategoryTabProps {
  active: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}

function CategoryTab({ active, icon, label, onClick }: CategoryTabProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm transition-colors',
        active
          ? 'bg-[hsl(var(--sidebar-surface-strong))] font-medium text-foreground'
          : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground'
      )}
    >
      {icon}
      {label}
    </button>
  );
}

interface SettingRowProps {
  label: string;
  description?: string;
  htmlFor?: string;
  children: ReactNode;
}

function SettingRow({ label, description, htmlFor, children }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <div className="min-w-0">
        <label htmlFor={htmlFor} className="text-sm font-medium text-foreground">
          {label}
        </label>
        {description && (
          <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

/**
 * Centralized Preferences panel (Typora-style: category rail + right-hand
 * settings). In-app modal rather than a separate OS window — all settings
 * live in per-webview zustand stores (useUIStore / useSettingsStore), so a
 * modal lets every change apply live without cross-window state bridging.
 * Shell modeled on Tabs/UnsavedCloseDialog.tsx; category rail modeled on
 * Sidebar.tsx's SidebarTab.
 */
export function PreferencesDialog() {
  const { t } = useTranslation();
  const open = useUIStore((state) => state.preferencesOpen);
  const setOpen = useUIStore((state) => state.setPreferencesOpen);
  const currentTheme = useUIStore((state) => state.currentTheme);
  const setCurrentTheme = useUIStore((state) => state.setCurrentTheme);
  const fontSize = useUIStore((state) => state.fontSize);
  const setFontSize = useUIStore((state) => state.setFontSize);

  const autoSave = useSettingsStore((state) => state.autoSave);
  const setAutoSave = useSettingsStore((state) => state.setAutoSave);
  const autoSaveInterval = useSettingsStore((state) => state.autoSaveInterval);
  const setAutoSaveInterval = useSettingsStore((state) => state.setAutoSaveInterval);
  const language = useSettingsStore((state) => state.language);
  const spellCheck = useSettingsStore((state) => state.spellCheck);
  const wordWrap = useSettingsStore((state) => state.wordWrap);
  const updateSettings = useSettingsStore((state) => state.updateSettings);

  const [category, setCategory] = useState<Category>('general');

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [open, setOpen]);

  if (!open) return null;

  const lightThemes = ALL_THEMES.filter((theme) => theme.variant === 'light');
  const darkThemes = ALL_THEMES.filter((theme) => theme.variant === 'dark');

  // App.tsx already has an effect that reacts to settingsStore.language
  // changes: it calls i18n.changeLanguage and syncs the choice to the Rust
  // backend (set_language + save_language_preference). Updating the store
  // here is all this control needs to do.
  const handleLanguageChange = (lang: string) => {
    updateSettings({ language: lang });
  };

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40"
      role="dialog"
      aria-modal="true"
      aria-labelledby="preferences-dialog-title"
      onClick={() => setOpen(false)}
    >
      <div
        className="flex h-[480px] w-full max-w-2xl overflow-hidden rounded-lg border bg-background shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Category rail */}
        <div className="flex w-44 flex-shrink-0 flex-col gap-1 border-r bg-[hsl(var(--sidebar-bg))] p-3">
          <h2 id="preferences-dialog-title" className="mb-2 px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            {t('preferences.title')}
          </h2>
          <CategoryTab
            active={category === 'general'}
            icon={<SettingsIcon className="h-3.5 w-3.5" />}
            label={t('preferences.section_general')}
            onClick={() => setCategory('general')}
          />
          <CategoryTab
            active={category === 'appearance'}
            icon={<Palette className="h-3.5 w-3.5" />}
            label={t('preferences.section_appearance')}
            onClick={() => setCategory('appearance')}
          />
          <CategoryTab
            active={category === 'editor'}
            icon={<Type className="h-3.5 w-3.5" />}
            label={t('preferences.section_editor')}
            onClick={() => setCategory('editor')}
          />
        </div>

        {/* Content */}
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="flex flex-shrink-0 items-center justify-between border-b px-5 py-3">
            <span className="text-sm font-semibold text-foreground">
              {category === 'general' && t('preferences.section_general')}
              {category === 'appearance' && t('preferences.section_appearance')}
              {category === 'editor' && t('preferences.section_editor')}
            </span>
            <button
              type="button"
              onClick={() => setOpen(false)}
              aria-label={t('preferences.close')}
              className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto px-5 py-2 divide-y divide-border">
            {category === 'general' && (
              <>
                <SettingRow
                  label={t('preferences.language')}
                  htmlFor="pref-language"
                >
                  <Select
                    id="pref-language"
                    value={language}
                    onChange={(e) => handleLanguageChange(e.target.value)}
                  >
                    <option value="en">English</option>
                    <option value="zh">繁體中文</option>
                  </Select>
                </SettingRow>
                <SettingRow
                  label={t('preferences.auto_save')}
                  description={t('preferences.auto_save_description')}
                  htmlFor="pref-auto-save"
                >
                  <Switch
                    id="pref-auto-save"
                    checked={autoSave}
                    onCheckedChange={setAutoSave}
                  />
                </SettingRow>
                <SettingRow
                  label={t('preferences.auto_save_interval')}
                  htmlFor="pref-auto-save-interval"
                >
                  <Select
                    id="pref-auto-save-interval"
                    value={String(autoSaveInterval)}
                    disabled={!autoSave}
                    onChange={(e) => setAutoSaveInterval(Number(e.target.value))}
                  >
                    <option value="2000">2s</option>
                    <option value="5000">5s</option>
                    <option value="10000">10s</option>
                    <option value="30000">30s</option>
                  </Select>
                </SettingRow>
              </>
            )}

            {category === 'appearance' && (
              <>
                <SettingRow label={t('preferences.theme')} htmlFor="pref-theme">
                  <Select
                    id="pref-theme"
                    value={currentTheme}
                    onChange={(e) => setCurrentTheme(e.target.value as ThemeName)}
                  >
                    <optgroup label={t('themes.light_themes')}>
                      {lightThemes.map((theme) => (
                        <option key={theme.name} value={theme.name}>
                          {theme.displayName}
                        </option>
                      ))}
                    </optgroup>
                    <optgroup label={t('themes.dark_themes')}>
                      {darkThemes.map((theme) => (
                        <option key={theme.name} value={theme.name}>
                          {theme.displayName}
                        </option>
                      ))}
                    </optgroup>
                  </Select>
                </SettingRow>
                <SettingRow label={t('preferences.font_size')} htmlFor="pref-font-size">
                  <Select
                    id="pref-font-size"
                    value={String(fontSize)}
                    onChange={(e) => setFontSize(Number(e.target.value))}
                  >
                    {[13, 14, 15, 16, 18, 20, 22].map((size) => (
                      <option key={size} value={size}>
                        {size}px
                      </option>
                    ))}
                  </Select>
                </SettingRow>
              </>
            )}

            {category === 'editor' && (
              <>
                <SettingRow
                  label={t('preferences.spell_check')}
                  htmlFor="pref-spell-check"
                >
                  <Switch
                    id="pref-spell-check"
                    checked={spellCheck}
                    onCheckedChange={(checked) => updateSettings({ spellCheck: checked })}
                  />
                </SettingRow>
                <SettingRow
                  label={t('preferences.word_wrap')}
                  description={t('preferences.word_wrap_description')}
                  htmlFor="pref-word-wrap"
                >
                  <Switch
                    id="pref-word-wrap"
                    checked={wordWrap}
                    onCheckedChange={(checked) => updateSettings({ wordWrap: checked })}
                  />
                </SettingRow>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
