/**
 * Layout calculation utilities for responsive markdown content
 */

export interface ContentCharacteristics {
  hasCodeBlocks: boolean;
  hasTables: boolean;
  hasImages: boolean;
  hasLists: boolean;
  hasBlockquotes: boolean;
  averageLineLength: number;
  estimatedHeight: number;
}

export interface ViewportInfo {
  width: number;
  height: number;
  dpr: number; // Device pixel ratio
  isMobile: boolean;
  isTablet: boolean;
  isDesktop: boolean;
}

export interface LayoutRecommendation {
  contentWidth: number;
  horizontalPadding: number;
  verticalPadding: number;
  lineHeight: number;
  fontSize: number;
  enableHorizontalScroll: boolean;
  columns: number;
}

/**
 * Get viewport information
 */
export function getViewportInfo(): ViewportInfo {
  const width = Math.max(
    document.documentElement.clientWidth || 0,
    window.innerWidth || 0
  );
  const height = Math.max(
    document.documentElement.clientHeight || 0,
    window.innerHeight || 0
  );
  const dpr = window.devicePixelRatio || 1;

  return {
    width,
    height,
    dpr,
    isMobile: width < 768,
    isTablet: width >= 768 && width < 1024,
    isDesktop: width >= 1024,
  };
}

/**
 * Calculate optimal content width based on viewport
 * Uses optimal line length of 65-75 characters for readability
 */
export function calculateOptimalContentWidth(
  viewportWidth: number,
  horizontalPadding: number = 16,
  scrollbarWidth: number = 15,
  maxWidth: number = 1024
): number {
  const availableWidth = viewportWidth - horizontalPadding - scrollbarWidth;
  return Math.min(availableWidth, maxWidth);
}

/**
 * Calculate optimal line height based on font size
 * Recommended ratio: 1.5-1.75 for body text
 */
export function calculateOptimalLineHeight(fontSize: number): number {
  if (fontSize < 14) return 1.6;
  if (fontSize < 16) return 1.65;
  if (fontSize < 18) return 1.7;
  return 1.75;
}

/**
 * Detect scrollbar presence based on element dimensions
 */
export function detectScrollbars(element: HTMLElement): {
  hasVertical: boolean;
  hasHorizontal: boolean;
  verticalScrollbarWidth: number;
} {
  const hasVertical = element.scrollHeight > element.clientHeight;
  const hasHorizontal = element.scrollWidth > element.clientWidth;
  const verticalScrollbarWidth = hasVertical ? 15 : 0; // Standard scrollbar width

  return {
    hasVertical,
    hasHorizontal,
    verticalScrollbarWidth,
  };
}

/**
 * Analyze content to determine layout characteristics
 */
export function analyzeContentCharacteristics(
  content: string
): ContentCharacteristics {
  const hasCodeBlocks = /```|```[\w-]*\n/g.test(content);
  const hasTables = /\|.*\|.*\n\|.*-.*\|/g.test(content);
  const hasImages = /!\[.*?\]\(.*?\)/g.test(content);
  const hasLists = /^[-*+]\s|^\d+\./gm.test(content);
  const hasBlockquotes = /^>/gm.test(content);

  // Calculate average line length
  const lines = content.split('\n').filter(line => line.trim());
  const averageLineLength = lines.length > 0
    ? lines.reduce((sum, line) => sum + line.length, 0) / lines.length
    : 0;

  // Estimate height (very rough approximation)
  const estimatedHeight = Math.ceil((lines.length * 24) + 200); // Rough estimate

  return {
    hasCodeBlocks,
    hasTables,
    hasImages,
    hasLists,
    hasBlockquotes,
    averageLineLength,
    estimatedHeight,
  };
}

/**
 * Get layout recommendation based on viewport and content
 */
export function getLayoutRecommendation(
  viewport: ViewportInfo,
  content: ContentCharacteristics,
  baseHorizontalPadding: number = 16
): LayoutRecommendation {
  let contentWidth: number;
  let horizontalPadding = baseHorizontalPadding;
  let verticalPadding: number;
  let lineHeight: number;
  let fontSize: number;
  let enableHorizontalScroll: boolean;
  const columns = 1;

  if (viewport.isMobile) {
    // Mobile: prioritize readability, maximize space
    contentWidth = Math.min(viewport.width - horizontalPadding * 2 - 30, 500);
    horizontalPadding = 8;
    verticalPadding = 16;
    lineHeight = 1.6;
    fontSize = 14;
    enableHorizontalScroll = true;

    // If content has code blocks or tables, enable scroll
    if (content.hasCodeBlocks || content.hasTables) {
      enableHorizontalScroll = true;
    }
  } else if (viewport.isTablet) {
    // Tablet: balanced layout
    contentWidth = Math.min(viewport.width - horizontalPadding * 2 - 30, 768);
    horizontalPadding = 12;
    verticalPadding = 20;
    lineHeight = 1.65;
    fontSize = 15;
    enableHorizontalScroll = content.hasCodeBlocks || content.hasTables;
  } else {
    // Desktop: optimal reading width
    contentWidth = 1024; // max-w-4xl
    horizontalPadding = 16;
    verticalPadding = 24;
    lineHeight = 1.75;
    fontSize = 16;
    enableHorizontalScroll = false;
  }

  return {
    contentWidth,
    horizontalPadding,
    verticalPadding,
    lineHeight,
    fontSize,
    enableHorizontalScroll,
    columns,
  };
}

/**
 * Calculate how many characters fit per line at a given width
 * Assumes monospace font for code blocks
 */
export function calculateCharactersPerLine(
  widthPx: number,
  charWidthPx: number = 9 // Average character width in prose
): number {
  return Math.floor(widthPx / charWidthPx);
}

/**
 * Determine if content needs horizontal scrolling
 */
export function needsHorizontalScroll(
  contentWidth: number,
  maxContentLineLength: number,
  charWidthPx: number = 9
): boolean {
  const requiredWidth = maxContentLineLength * charWidthPx;
  return requiredWidth > contentWidth;
}

/**
 * Calculate scrollable area dimensions for code blocks/tables
 */
export function calculateScrollableAreaDimensions(
  element: HTMLElement,
  maxWidth: number
) {
  return {
    width: Math.min(element.scrollWidth, maxWidth),
    height: element.scrollHeight,
    isScrollableX: element.scrollWidth > maxWidth,
    isScrollableY: element.scrollHeight > (element.parentElement?.clientHeight || 0),
    scrollbarWidthX: element.scrollWidth > maxWidth ? 15 : 0,
    scrollbarWidthY: element.scrollHeight > (element.parentElement?.clientHeight || 0) ? 15 : 0,
  };
}

/**
 * Debounce function for layout recalculations
 */
export function debounceLayoutCalculation(
  callback: () => void,
  delay: number = 100
): (() => void) {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return () => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => {
      callback();
      timeoutId = null;
    }, delay);
  };
}

/**
 * Throttle function for continuous events like scroll
 */
export function throttleLayoutCalculation(
  callback: () => void,
  delay: number = 100
): (() => void) {
  let lastCall = 0;

  return () => {
    const now = Date.now();
    if (now - lastCall >= delay) {
      callback();
      lastCall = now;
    }
  };
}

/**
 * Get CSS media query breakpoints
 */
export const BREAKPOINTS = {
  xs: 0,      // Extra small (mobile)
  sm: 640,    // Small (landscape mobile)
  md: 768,    // Medium (tablet)
  lg: 1024,   // Large (desktop)
  xl: 1280,   // Extra large (wide desktop)
  '2xl': 1536, // 2x Extra large (ultra-wide)
} as const;

/**
 * Optimal content widths for different breakpoints
 */
export const OPTIMAL_WIDTHS = {
  mobile: 500,
  tablet: 768,
  desktop: 1024,
  ultrawide: 1280,
} as const;

/**
 * Font size recommendations
 */
export const FONT_SIZES = {
  mobile: { prose: 14, code: 12 },
  tablet: { prose: 15, code: 13 },
  desktop: { prose: 16, code: 14 },
} as const;

/**
 * Line height recommendations for readability
 */
export const LINE_HEIGHTS = {
  tight: 1.5,
  normal: 1.65,
  relaxed: 1.75,
  loose: 1.85,
} as const;
