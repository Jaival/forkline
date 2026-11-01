// Helpers for the bench scenario and for views to signal "content shown".

/** Resolves after the browser has painted the current frame. */
export function afterPaint(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
}

const waiters = new Map<string, Array<() => void>>();
const ready = new Set<string>();

/**
 * Views call this once their real content is in the DOM, with a key like
 * "home" or "issue:123". Cheap no-op when nobody waits.
 */
export function signalReady(key: string): void {
  ready.add(key);
  const list = waiters.get(key);
  if (list) {
    waiters.delete(key);
    for (const w of list) w();
  }
}

export function waitReady(key: string): Promise<void> {
  if (ready.has(key)) return Promise.resolve();
  return new Promise((resolve) => {
    const list = waiters.get(key) ?? [];
    list.push(resolve);
    waiters.set(key, list);
  });
}

/** Forget a key so the next `waitReady` waits for a fresh signal. */
export function resetReady(key: string): void {
  ready.delete(key);
}
