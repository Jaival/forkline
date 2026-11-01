import { type ViewRef, viewTitle } from "../../lib/layout";

export function Placeholder({ view }: { view: ViewRef }) {
  return (
    <section className="p-6">
      <h1 className="text-xl font-semibold">{viewTitle(view)}</h1>
      <p className="mt-2 text-muted">This view is scaffolded for a later phase.</p>
    </section>
  );
}
