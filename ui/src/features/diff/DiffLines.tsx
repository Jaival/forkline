import { useVirtualizer } from "@tanstack/react-virtual";
import { forwardRef, useImperativeHandle, useMemo, useRef } from "react";
import type { DiffLine, DiffSide, DraftReviewComment } from "../../bindings";
import { Composer, DraftCard, ThreadCard } from "./ThreadCard";
import { lineAnchor, type DiffRow } from "./rows";

export const LINE_HEIGHT = 20;

const LINE_BG = {
  add: "bg-open/15",
  del: "bg-closed/15",
  context: "",
} as const;

const MARK = { add: "+", del: "-", context: " " } as const;

export type DiffLinesHandle = { scrollEl: HTMLDivElement | null };

/** Lines `from..=to` on `side` are marked as the range being commented on. */
export type LineRange = { side: DiffSide; from: number; to: number };

/**
 * Virtualized unified diff. Code lines and hunk headers have a fixed height;
 * thread, draft and composer rows are measured. Long lines scroll
 * horizontally; cards and hunk headers stay pinned to the visible width.
 */
export const DiffLines = forwardRef<
  DiffLinesHandle,
  {
    rows: DiffRow[];
    range?: LineRange | null;
    /** Gutter click; `extend` is true with Shift held. */
    onOpenComposer?: (side: DiffSide, line: number, extend: boolean) => void;
    onCloseComposer?: () => void;
    onSaveDraft?: (body: string) => void;
    onRemoveDraft?: (d: DraftReviewComment) => void;
  }
>(function DiffLines({ rows, range, onOpenComposer, onCloseComposer, onSaveDraft, onRemoveDraft }, ref) {
  const scrollRef = useRef<HTMLDivElement>(null);
  useImperativeHandle(ref, () => ({ get scrollEl() { return scrollRef.current; } }), []);
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (rows[i]?.kind === "line" || rows[i]?.kind === "hunk" ? LINE_HEIGHT : 120),
    overscan: 20,
  });
  // Widest line in characters: sets the scroll width so no line is clipped.
  const widest = useMemo(() => rows.reduce((m, r) => (r.kind === "line" ? Math.max(m, r.line.text.length) : m), 0), [rows]);
  const inRange = (l: DiffLine) => {
    if (!range) return false;
    const a = lineAnchor(l);
    return a !== null && a.side === range.side && a.line >= range.from && a.line <= range.to;
  };

  return (
    <div
      ref={scrollRef}
      className="h-full overflow-auto font-mono text-xs"
      style={{ containerType: "inline-size" }}
      data-testid="diff-scroll"
    >
      <div className="relative" style={{ height: virt.getTotalSize(), minWidth: `max(100%, calc(${widest}ch + 7rem))` }}>
        {virt.getVirtualItems().map((v) => {
          const row = rows[v.index];
          if (!row) return null;
          const fixed = row.kind === "line" || row.kind === "hunk";
          return (
            <div
              key={v.key}
              data-index={v.index}
              ref={fixed ? undefined : virt.measureElement}
              className="absolute left-0 top-0 w-full"
              style={{ transform: `translateY(${v.start}px)`, height: fixed ? LINE_HEIGHT : undefined }}
            >
              {row.kind === "hunk" ? (
                <div className="sticky left-0 h-full w-[100cqw] truncate bg-raised px-3 leading-5 text-muted">{row.text}</div>
              ) : row.kind === "line" ? (
                <div className={`flex h-full leading-5 ${LINE_BG[row.line.kind]} ${inRange(row.line) ? "shadow-[inset_3px_0_0_var(--fl-accent)]" : ""}`}>
                  {[row.line.old_no, row.line.new_no].map((n, i) => (
                    <button
                      key={i}
                      className="w-12 shrink-0 select-none pr-2 text-right text-muted hover:text-accent"
                      tabIndex={-1}
                      title="Comment on this line (Shift+click: select a range)"
                      onClick={(e) => {
                        const a = lineAnchor(row.line);
                        if (a && onOpenComposer) onOpenComposer(a.side, a.line, e.shiftKey);
                      }}
                    >
                      {n ?? ""}
                    </button>
                  ))}
                  <span className="w-4 shrink-0 select-none text-muted">{MARK[row.line.kind]}</span>
                  <span className="flex-1 whitespace-pre">{row.line.text}</span>
                </div>
              ) : (
                <div className="sticky left-0 w-[100cqw]">
                  {row.kind === "thread" ? (
                    <ThreadCard thread={row.thread} />
                  ) : row.kind === "draft" ? (
                    <DraftCard draft={row.draft} onRemove={() => onRemoveDraft?.(row.draft)} />
                  ) : (
                    <Composer
                      startLine={row.startLine}
                      line={row.line}
                      onCancel={() => onCloseComposer?.()}
                      onSave={(body) => onSaveDraft?.(body)}
                    />
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
});
