import { type ReactNode, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

/**
 * Shared virtual list (TanStack Virtual, fixed row height).
 *
 * - `items`, `rowHeight`: rows are `rowHeight` px tall.
 * - `renderRow(item, index)`: return the row content; the list positions it.
 * - `getKey(item)`: stable key per row.
 * - `onEndReached`: called when the last ~5 rows are visible; use it to fetch
 *   the next page. Safe to call repeatedly (guard with `isFetching`).
 * - `label`: accessible name of the list.
 */
export function VirtualList<T>({
  items,
  rowHeight,
  renderRow,
  getKey,
  onEndReached,
  label,
}: {
  items: T[];
  rowHeight: number;
  renderRow: (item: T, index: number) => ReactNode;
  getKey: (item: T) => string | number;
  onEndReached?: () => void;
  label: string;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  // TanStack Virtual returns unmemoizable functions; this component does not memoize them.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 8,
    getItemKey: (i) => getKey(items[i]!),
    // Rows render before the first measurement, e.g. in a hidden tab.
    initialRect: { width: 800, height: 800 },
  });
  const virtualItems = virtualizer.getVirtualItems();
  const last = virtualItems.at(-1)?.index ?? -1;

  useEffect(() => {
    if (onEndReached && last >= items.length - 5) onEndReached();
  }, [last, items.length, onEndReached]);

  return (
    <div ref={scrollRef} role="list" aria-label={label} className="h-full overflow-auto">
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualItems.map((v) => (
          <div
            key={v.key}
            role="listitem"
            data-index={v.index}
            className="absolute top-0 left-0 w-full"
            style={{ height: rowHeight, transform: `translateY(${v.start}px)` }}
          >
            {renderRow(items[v.index]!, v.index)}
          </div>
        ))}
      </div>
    </div>
  );
}
