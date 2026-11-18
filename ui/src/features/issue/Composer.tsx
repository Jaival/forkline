import { useState } from "react";
import { Markdown } from "./Markdown";

/** Comment box with a markdown preview tab. */
export function Composer({
  disabled,
  onSubmit,
}: {
  disabled: boolean;
  onSubmit: (body: string) => Promise<boolean>;
}) {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState(false);
  const submit = async () => {
    if (await onSubmit(text.trim())) setText("");
  };
  return (
    <section aria-label="New comment" className="rounded border border-border">
      <div className="flex gap-1 border-b border-border bg-panel px-2 py-1">
        <button className={`rounded px-2 ${preview ? "" : "bg-raised"}`} onClick={() => setPreview(false)}>Write</button>
        <button className={`rounded px-2 ${preview ? "bg-raised" : ""}`} onClick={() => setPreview(true)}>Preview</button>
      </div>
      {preview ? (
        <div className="min-h-24 p-3">
          {text.trim() ? <Markdown html={null} md={text} /> : <p className="text-muted">Nothing to preview.</p>}
        </div>
      ) : (
        <textarea
          aria-label="Comment"
          className="block min-h-24 w-full resize-y bg-bg p-3 outline-none"
          placeholder="Leave a comment (markdown)"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      )}
      <div className="flex justify-end border-t border-border p-2">
        <button
          className="rounded bg-accent px-3 py-1 text-white disabled:opacity-50"
          disabled={disabled || !text.trim()}
          onClick={() => void submit()}
        >
          Comment
        </button>
      </div>
    </section>
  );
}
