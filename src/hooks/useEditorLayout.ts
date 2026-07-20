import { useEffect, useRef, useState } from 'react';

interface LayoutMetrics {
  viewportWidth: number;
  viewportHeight: number;
  availableWidth: number;
  availableHeight: number;
  contentWidth: number;
  horizontalPadding: number;
  verticalPadding: number;
  hasVerticalScrollbar: boolean;
  hasHorizontalScrollbar: boolean;
  optimalLineLength: number;
}

/**
 * Hook for calculating editor layout metrics and responsive sizing.
 * Measures viewport, accounts for scrollbars, and calculates optimal content width.
 */
export function useEditorLayout(containerRef: React.RefObject<HTMLDivElement>) {
  const [metrics, setMetrics] = useState<LayoutMetrics>({
    viewportWidth: 0,
    viewportHeight: 0,
    availableWidth: 0,
    availableHeight: 0,
    contentWidth: 0,
    horizontalPadding: 16, // px-4 = 8px * 2
    verticalPadding: 24, // py-6 = 6 * 4px = 24px
    hasVerticalScrollbar: false,
    hasHorizontalScrollbar: false,
    optimalLineLength: 0,
  });

  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const frameRef = useRef<number | null>(null);
  const lastMetricsRef = useRef<LayoutMetrics | null>(null);

  useEffect(() => {
    const calculateMetrics = () => {
      if (!containerRef.current) return;

      const container = containerRef.current;
      const viewportWidth = container.clientWidth;
      const viewportHeight = container.clientHeight;

      // Bail out (keep the last real metrics) when the container is
      // zero-sized — this happens while its pane is `hidden` (EditorHost
      // keeps inactive tabs/modes mounted but `display:none`). Without this,
      // `availableWidth` goes negative and `contentWidth` collapses to 0,
      // which then forces a visible reflow (and disturbs scroll restore)
      // when the pane becomes visible again and this hook's ResizeObserver
      // fires with the real size.
      if (viewportWidth === 0 && viewportHeight === 0) return;

      // Detect scrollbars
      const hasVerticalScrollbar = container.scrollHeight > container.clientHeight;
      const hasHorizontalScrollbar = container.scrollWidth > container.clientWidth;

      // Account for scrollbar width (typically 15px on most browsers)
      const scrollbarWidth = hasVerticalScrollbar ? 15 : 0;

      // Calculate available width after accounting for padding and scrollbars
      const horizontalPadding = 16; // px-4 = 8px * 2
      const availableWidth = viewportWidth - horizontalPadding - scrollbarWidth;

      // Calculate available height after padding
      const verticalPadding = 24; // py-6 = 6 * 4px
      const availableHeight = viewportHeight - verticalPadding;

      // Determine optimal content width
      // For readability, aim for 65-75 characters per line (approximately 600-800px)
      // Start with available width, cap at max-w-4xl (1024px)
      const maxContentWidth = 1024; // max-w-4xl
      const contentWidth = Math.min(availableWidth, maxContentWidth);

      // Calculate optimal line length for better readability
      // Assuming average character width of ~8-10px for prose text
      const avgCharWidth = 9;
      const optimalLineLength = Math.round(contentWidth / avgCharWidth);

      const nextMetrics: LayoutMetrics = {
        viewportWidth,
        viewportHeight,
        availableWidth,
        availableHeight,
        contentWidth,
        horizontalPadding,
        verticalPadding,
        hasVerticalScrollbar,
        hasHorizontalScrollbar,
        optimalLineLength,
      };

      const prevMetrics = lastMetricsRef.current;
      const isSame =
        prevMetrics &&
        prevMetrics.viewportWidth === nextMetrics.viewportWidth &&
        prevMetrics.viewportHeight === nextMetrics.viewportHeight &&
        prevMetrics.availableWidth === nextMetrics.availableWidth &&
        prevMetrics.availableHeight === nextMetrics.availableHeight &&
        prevMetrics.contentWidth === nextMetrics.contentWidth &&
        prevMetrics.horizontalPadding === nextMetrics.horizontalPadding &&
        prevMetrics.verticalPadding === nextMetrics.verticalPadding &&
        prevMetrics.hasVerticalScrollbar === nextMetrics.hasVerticalScrollbar &&
        prevMetrics.hasHorizontalScrollbar === nextMetrics.hasHorizontalScrollbar &&
        prevMetrics.optimalLineLength === nextMetrics.optimalLineLength;

      if (!isSame) {
        lastMetricsRef.current = nextMetrics;
        setMetrics(nextMetrics);
      }
    };

    // Schedule calculation on next animation frame for smoother updates
    const scheduleCalculate = () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
      }
      frameRef.current = requestAnimationFrame(() => {
        frameRef.current = null;
        calculateMetrics();
      });
    };

    // Initial calculation
    calculateMetrics();

    // Set up ResizeObserver for container changes
    resizeObserverRef.current = new ResizeObserver(scheduleCalculate);
    if (containerRef.current) {
      resizeObserverRef.current.observe(containerRef.current);
    }

    // Listen for window resize events
    window.addEventListener('resize', scheduleCalculate);

    return () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
      }
      if (resizeObserverRef.current) {
        resizeObserverRef.current.disconnect();
      }
      window.removeEventListener('resize', scheduleCalculate);
    };
  }, [containerRef]);

  return metrics;
}
