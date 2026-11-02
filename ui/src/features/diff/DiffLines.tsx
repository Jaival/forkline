import { useVirtualizer } from "@tanstack/react-virtual";
import { forwardRef, useImperativeHandle, useRef } from "react";
import type { DiffSide, DraftReviewComment } from "../../bindings";
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

/**
 * Virtualized unified diff. Code lines and hunk headers have a fixed height;
 * thread, draft and composer rows are measured.
 */
export const DiffLines = forwardRef<
  DiffLinesHandle,
  {
    rows: DiffRow[];
    onOpenComposer?: (side: DiffSide, line: number) => void;
    onCloseComposer?: () => void;
    onSaveDraft?: (side: DiffSide, line: number, body: string) => void;
    onRemoveDraft?: (d: DraftReviewComment) => void;
  }
>(function DiffLines({ rows, onOpenComposer, onCloseComposer, onSaveDraft, onRemoveDraft }, ref) {
  const scrollRef = useRef<HTMLDivElement>(null);
  useImperativeHandle(ref, () => ({ get scrollEl() { return scrollRef.current; } }), []);
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (rows[i]?.kind === "line" || rows[i]?.kind === "hunk" ? LINE_HEIGHT : 120),
    overscan: 20,
  });

  return (
    <div ref={scrollRef} className="h-full overflow-auto font-mono text-xs" data-testid="diff-scroll">
      <div className="relative w-full" style={{ height: virt.getTotalSize() }}>
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
                <div className="h-full truncate bg-raised px-3 leading-5 text-muted">{row.text}</div>
              ) : row.kind === "line" ? (
                <div className={`flex h-full leading-5 ${LINE_BG[row.line.kind]}`}>
                  {[row.line.old_no, row.line.new_no].map((n, i) => (
                    <button
                      key={i}
                      className="w-12 shrink-0 select-none pr-2 text-right text-muted hover:text-accent"
                      tabIndex={-1}
                      onClick={() => {
                        const a = lineAnchor(row.line);
                        if (a && onOpenComposer) onOpenComposer(a.side, a.line);
                      }}
                    >
                      {n ?? ""}
                    </button>
                  ))}
                  <span className="w-4 shrink-0 select-none text-muted">{MARK[row.line.kind]}</span>
                  <span className="min-w-0 flex-1 overflow-hidden whitespace-pre">{row.line.text}</span>
                </div>
              ) : row.kind === "thread" ? (
                <ThreadCard thread={row.thread} />
              ) : row.kind === "draft" ? (
                <DraftCard draft={row.draft} onRemove={() => onRemoveDraft?.(row.draft)} />
              ) : (
                <Composer
                  onCancel={() => onCloseComposer?.()}
                  onSave={(body) => onSaveDraft?.(row.side, row.line, body)}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
});
