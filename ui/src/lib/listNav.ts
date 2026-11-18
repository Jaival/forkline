// listNav: keyboard selection for the active list (j / k / Enter).
//
// Usage in a list view:
//
//   const { selected, setSelected, onKeyDown } = useListNav({
//     count: rows.length,
//     onOpen: (i) => open(rows[i]),
//     onSelect: (i) => virtualizer.scrollToIndex(i),   // optional
//   });
//   <div tabIndex={0} onKeyDown={onKeyDown}> ... highlight row `selected` ... </div>
//
// - `j` / ArrowDown moves down, `k` / ArrowUp moves up (clamped, no wrap).
// - `Enter` calls `onOpen(selected)`. `Home` / `End` jump to first / last.
// - Keys are ignored while typing in an input, textarea or select, and when
//   a modifier key is held.
// - `selected` is -1 until the user moves. Selection resets when `count` hits 0.
//
// The hook only handles keys that reach the element it is attached to, so the
// pane that owns focus is the "active list". The global keymap (shell/keymap.ts)
// focuses the active view's `[data-list-nav]` element when `j` / `k` is pressed
// with nothing focused: put `data-list-nav` on the element that gets `onKeyDown`.

import { type KeyboardEvent, useCallback, useState } from "react";

export type ListNavOptions = {
  count: number;
  onOpen: (index: number) => void;
  /** Called after the selection changes, e.g. to scroll the row into view. */
  onSelect?: (index: number) => void;
};

/** Pure: the next selected index for a key, or null if the key is not handled. */
export function nextIndex(key: string, selected: number, count: number): number | null {
  if (count <= 0) return null;
  switch (key) {
    case "j":
    case "ArrowDown":
      return Math.min(selected + 1, count - 1);
    case "k":
    case "ArrowUp":
      return Math.max(selected - 1, 0);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

export function isTypingTarget(t: EventTarget | null): boolean {
  return t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
}

export function useListNav({ count, onOpen, onSelect }: ListNavOptions) {
  const [raw, setSelected] = useState(-1);
  const selected = Math.min(raw, count - 1);

  const onKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.ctrlKey || e.altKey || e.metaKey || isTypingTarget(e.target)) return;
      if (e.key === "Enter") {
        if (selected >= 0) {
          onOpen(selected);
          e.preventDefault();
        }
        return;
      }
      const next = nextIndex(e.key, selected, count);
      if (next === null) return;
      e.preventDefault();
      setSelected(next);
      onSelect?.(next);
    },
    [selected, count, onOpen, onSelect],
  );

  return { selected, setSelected, onKeyDown };
}
