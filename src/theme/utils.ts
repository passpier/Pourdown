import { ThemeName, THEME_NAMES } from './types';

/**
 * Convert CSS hex color to HSL for Tailwind CSS variables
 */
export function hexToHSL(hex: string): string {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (!result) return '0 0% 0%';

  const r = parseInt(result[1], 16) / 255;
  const g = parseInt(result[2], 16) / 255;
  const b = parseInt(result[3], 16) / 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;

  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);

    switch (max) {
      case r:
        h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
        break;
      case g:
        h = ((b - r) / d + 2) / 6;
        break;
      case b:
        h = ((r - g) / d + 4) / 6;
        break;
    }
  }

  const hue = Math.round(h * 360);
  const saturation = Math.round(s * 100);
  const lightness = Math.round(l * 100);

  return `${hue} ${saturation}% ${lightness}%`;
}

/**
 * Apply theme CSS variables to the document
 */
export function applyTheme(themeName: ThemeName): void {
  const theme = THEME_NAMES[themeName];
  if (!theme) return;

  const root = document.documentElement;
  const colors = theme.colors;

  // Set CSS custom properties
  const setCSSVar = (name: string, value: string) => {
    root.style.setProperty(name, hexToHSL(value));
  };

  // Background colors
  setCSSVar('--bg-primary', colors.bgPrimary);
  setCSSVar('--bg-secondary', colors.bgSecondary);
  setCSSVar('--bg-code', colors.bgCode);
  setCSSVar('--bg-blockquote', colors.bgBlockquote);
  setCSSVar('--bg-inline-code', colors.bgInlineCode);
  setCSSVar('--bg-selection', colors.bgSelection);

  // Text colors
  setCSSVar('--text-primary', colors.textPrimary);
  setCSSVar('--text-heading', colors.textHeading);
  setCSSVar('--text-muted', colors.textMuted);

  // Link colors
  setCSSVar('--link-color', colors.linkColor);
  setCSSVar('--link-hover', colors.linkHover);
  setCSSVar('--link-visited', colors.linkVisited);

  // Border colors
  setCSSVar('--border-color', colors.borderColor);
  setCSSVar('--border-muted', colors.borderMuted);
  setCSSVar('--border-blockquote', colors.borderBlockquote);

  // Syntax highlighting
  setCSSVar('--syntax-keyword', colors.syntaxKeyword);
  setCSSVar('--syntax-string', colors.syntaxString);
  setCSSVar('--syntax-number', colors.syntaxNumber);
  setCSSVar('--syntax-comment', colors.syntaxComment);
  setCSSVar('--syntax-function', colors.syntaxFunction);
  setCSSVar('--syntax-variable', colors.syntaxVariable);
  setCSSVar('--syntax-operator', colors.syntaxOperator);
  setCSSVar('--syntax-class', colors.syntaxClass);
  setCSSVar('--syntax-tag', colors.syntaxTag);

  // Special elements
  setCSSVar('--table-header-bg', colors.tableHeaderBg);
  setCSSVar('--table-header-text', colors.tableHeaderText);
  setCSSVar('--table-row-alt', colors.tableRowAlt);
  setCSSVar('--checkbox-checked', colors.checkboxChecked);
  setCSSVar('--checkbox-unchecked', colors.checkboxUnchecked);

  // Update Tailwind's base colors to match theme
  setCSSVar('--background', colors.bgPrimary);
  setCSSVar('--foreground', colors.textPrimary);
  setCSSVar('--border', colors.borderColor);
  setCSSVar('--muted-foreground', colors.textMuted);
  setCSSVar('--popover', colors.bgPrimary);
  setCSSVar('--popover-foreground', colors.textPrimary);
  setCSSVar('--accent', colors.bgSecondary);
  setCSSVar('--accent-foreground', colors.textPrimary);
  setCSSVar('--muted', colors.bgSecondary);

  // Sidebar (paper cards) and code line-number vars — previously hardcoded
  // to light-mode values in index.css and never updated per theme.
  setCSSVar('--sidebar-bg', colors.bgSecondary);
  setCSSVar('--sidebar-surface', colors.bgPrimary);
  setCSSVar('--sidebar-surface-strong', colors.bgSecondary);
  setCSSVar('--sidebar-border', colors.borderColor);
  setCSSVar('--sidebar-highlight-bg', colors.linkColor);
  setCSSVar('--line-highlight-bg', colors.borderMuted);
  setCSSVar('--line-number-color', colors.textMuted);

  // Prose and code font stacks (raw font-family strings, not colors — must
  // NOT go through hexToHSL). Drive .tiptap (prose) and code fonts in
  // index.css, CodeBlockRenderer.css, and SourceEditor.tsx respectively.
  root.style.setProperty('--font-body', theme.fontBody);
  root.style.setProperty('--font-code', theme.fontCode);

  // Toggle Tailwind's `.dark` class and native color-scheme so `dark:`
  // utilities and native form controls/scrollbars follow the selected
  // theme (the app never switches theme via OS preference alone).
  root.classList.toggle('dark', theme.variant === 'dark');
  root.style.colorScheme = theme.variant;
}

/**
 * Detect system color scheme preference
 */
export function detectSystemTheme(): 'light' | 'dark' {
  if (typeof window === 'undefined') return 'light';

  const darkModeQuery = window.matchMedia('(prefers-color-scheme: dark)');
  return darkModeQuery.matches ? 'dark' : 'light';
}

/**
 * Get the appropriate default theme based on system preference
 */
export function getDefaultTheme(systemTheme: 'light' | 'dark'): ThemeName {
  return systemTheme === 'dark' ? 'one-dark-pro' : 'github-light';
}

/**
 * Listen for system theme changes
 */
export function listenToSystemThemeChanges(callback: (theme: 'light' | 'dark') => void): () => void {
  if (typeof window === 'undefined') return () => {};

  const darkModeQuery = window.matchMedia('(prefers-color-scheme: dark)');
  
  const handler = (e: MediaQueryListEvent) => {
    callback(e.matches ? 'dark' : 'light');
  };

  darkModeQuery.addEventListener('change', handler);

  return () => {
    darkModeQuery.removeEventListener('change', handler);
  };
}

/**
 * Listen for prefers-reduced-motion
 */
export function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined') return false;
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}
