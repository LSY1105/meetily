# Fix: Tauri dev-mode `listen()` hydration race

## Problem

Running `pnpm tauri:dev` produces 19+ runtime errors of the form:

```
TypeError: Cannot read properties of undefined (reading 'transformCallback')
```

Source: every `await listen<...>('event-name', cb)` call inside a `useEffect`.
Affecting 20 files (e.g. `ClientRootLayout.tsx`, `useModalState.ts`, `useImportAudio.ts`,
`DownloadProgressToast.tsx`, `OnboardingContext.tsx`, `Sidebar/index.tsx`, `ConfigContext.tsx`).

Production builds (`tauri build`) are unaffected — the binary starts and the user
reaches Settings / onboarding without errors. This is dev-mode only.

## Root cause

In Tauri 2.x the webview loads the dev URL (`http://localhost:3118`) and the
`window.__TAURI_INTERNALS__` bridge object is injected by a runtime script that
runs **after** the page's first JS executes. React's first `useEffect` flush
(in StrictMode-disabled rendering, which this app uses for BlockNote) fires before
injection completes, so `listen()` reads
`window.__TAURI_INTERNALS__.transformCallback` on an undefined object.

`app.withGlobalTauri: true` does NOT solve this — that flag controls
`window.__TAURI__`, not `__TAURI_INTERNALS__`. The internals bridge is internal
and not gated by the config flag.

The production binary embeds the frontend assets inside the webview, so the
injection script runs synchronously before any user code.

## Fix (Ponytail: shortest diff)

Add **one** shim that waits for `__TAURI_INTERNALS__` to appear (≤500 ms) before
delegating to the real `listen()`. Replace the import in all 20 files.

### New file: `frontend/src/lib/transport.ts`

```ts
import { listen as tauriListen, type UnlistenFn, type EventCallback } from '@tauri-apps/api/event';

const TAURI_INTERNALS = '__TAURI_INTERNALS__' as const;
const MAX_WAIT_MS = 500;
const POLL_MS = 25;

const hasInternals = () =>
  typeof window !== 'undefined' && !!(window as unknown as Record<string, unknown>)[TAURI_INTERNALS];

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
    // Outside Tauri (web preview, SSR) or the bridge never came up — no-op.
    console.warn(`[transport] Tauri bridge missing; dropping listener "${event}"`);
    return () => {};
  }
  return tauriListen<T>(event, handler);
}
```

### Edits: 20 files

In every file that does:

```ts
import { listen, UnlistenFn } from '@tauri-apps/api/event';
// or
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
```

change to:

```ts
import { listen } from '@/lib/transport';
```

`UnlistenFn` is now imported from `'@tauri-apps/api/event'` only where still
needed (most files keep `UnlistenFn` as a type and import it from
`'@tauri-apps/api/event'`; some no longer need it after this change).

Mechanical regex:

```
From:    import \{ listen(, type? UnlistenFn)? \} from '@tauri-apps/api/event';
To:      import \{ listen \} from '@/lib/transport';
```

Then for any file that still references `UnlistenFn` and no longer has a
`listen` import from `@tauri-apps/api/event`, add a separate
`import type { UnlistenFn } from '@tauri-apps/api/event';`.

## Scope

- 1 new file (~25 lines, zero deps)
- 20 import-line edits (1 line each, no logic change)
- 0 backend changes
- 0 new i18n keys
- 0 new dependencies
- 0 behavior change in production (bridge is up on frame 0; shim passes through)

## Non-goals

- Not wrapping `invoke()` — not in the error report, works in dev today.
- Not wrapping `transformCallback` consumers manually per call site — one shim
  for 20 callers beats 20 guards.
- Not adding telemetry / reconnection logic — out of scope; the bridge, once
  present, stays present for the process lifetime.

## Verification

Local:

```
pnpm install
pnpm tauri:dev
```

Confirm:

1. DevTools console shows zero `transformCallback` errors.
2. Onboarding / download / sidebar / recording flows still receive their events
   (manual smoke; spec'd per Wave 29-30 PR-45b which already validated the
   underlying hooks).
3. `pnpm build` (CI's `i18n-check` job) passes — the shim is SSR-safe
   (`typeof window` guard) and `output: 'export'` stays unchanged.

## Risk

Minimal. Shim is a thin wrapper; production path is one await + one delegate.
Worst case if the bridge truly never arrives (genuine non-Tauri browser):
listeners silently no-op, which is the same behavior as today — those events
have no meaning outside Tauri anyway.
