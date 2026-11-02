import { type ReactNode, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useListNav } from "./listNav";

/**
 * Shared virtual list (TanStack Virtual, fixed row height).
 *
 * - `items`, `rowHeight`: rows are `rowHeight` px tall.
 * - `renderRow(item, index)`: return the row content; the list positions it.
 * - `getKey(item)`: stable key per row.
 * - `onEndReached`: called when the last ~5 rows are visible; use it to fetch
 *   the next page. Safe to call repeatedly (guard with `isFetching`).
 * - `label`: accessible name of the list.
 * - `onOpen(item)`: enables keyboard navigation (`j` / `k` / Enter, see
 *   `listNav.ts`); Enter opens the selected row. Enter does nothing on rows
 *   where `canSelect` returns false, such as group headers.
 */
export function VirtualList<T>({
  items,
  rowHeight,
  renderRow,
  getKey,
  onEndReached,
  label,
  onOpen,
  canSelect,
}: {
  items: T[];
  rowHeight: number;
  renderRow: (item: T, index: number) => ReactNode;
  getKey: (item: T) => string | number;
  onEndReached?: () => void;
  label: string;
  onOpen?: (item: T) => void;
  canSelect?: (item: T) => boolean;
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
  const nav = useListNav({
    count: items.length,
    onOpen: (i) => {
      const item = items[i];
      if (item !== undefined && onOpen && (canSelect?.(item) ?? true)) onOpen(item);
    },
    onSelect: (i) => virtualizer.scrollToIndex(i),
  });
  const virtualItems = virtualizer.getVirtualItems();
  const last = virtualItems.at(-1)?.index ?? -1;

  useEffect(() => {
    if (onEndReached && last >= items.length - 5) onEndReached();
  }, [last, items.length, onEndReached]);

  return (
    <div
      ref={scrollRef}
      role="list"
      aria-label={label}
      className="h-full overflow-auto outline-none"
      {...(onOpen ? { tabIndex: 0, onKeyDown: nav.onKeyDown, "data-list-nav": true } : {})}
    >
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualItems.map((v) => (
          <div
            key={v.key}
            role="listitem"
            data-index={v.index}
            aria-current={v.index === nav.selected || undefined}
            className={`absolute top-0 left-0 w-full ${v.index === nav.selected ? "bg-raised" : ""}`}
            style={{ height: rowHeight, transform: `translateY(${v.start}px)` }}
          >
            {renderRow(items[v.index]!, v.index)}
          </div>
        ))}
      </div>
    </div>
  );
}
