import { listen as tauriListen, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';

const TAURI_INTERNALS = '__TAURI_INTERNALS__' as const;
const MAX_WAIT_MS = 500;
const POLL_MS = 25;

const hasInternals = (): boolean =>
  typeof window !== 'undefined' &&
  !!(window as unknown as Record<string, unknown>)[TAURI_INTERNALS];

async function waitForTauri(): Promise<boolean> {
  if (hasInternals()) return true;
  const start = Date.now();
  while (Date.now() - start < MAX_WAIT_MS) {
    await new Promise((r) => setTimeout(r, POLL_MS));
    if (hasInternals()) return true;
  }
  return false;
}

export async function listen<T>(
  event: string,
  handler: EventCallback<T>,
): Promise<UnlistenFn> {
  if (!(await waitForTauri())) {
    console.warn(`[transport] Tauri bridge missing; dropping listener "${event}"`);
    return () => {};
  }
  return tauriListen<T>(event, handler);
}
