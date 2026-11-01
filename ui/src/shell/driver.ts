import type { Layout, LayoutAction } from "../lib/layout";

/**
 * Handle to the live layout for code outside React (bench scenario,
 * command palette). Set by <Shell> on every render.
 */
export const driver: { dispatch?: (a: LayoutAction) => void; layout?: Layout } = {};
