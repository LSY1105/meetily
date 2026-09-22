# `src/lib/` — Tauri bindings placeholder

## Current state

`bindings.ts` is a hand-written placeholder that mirrors the Tauri commands
defined in `crates/qmeetily-app/src/commands.rs`. Frontend code should
import from here:

```ts
import { commands, type AppInfo, type Meeting } from '@/lib/bindings';

const info = await commands.getAppInfo();      // typed as AppInfo
const list = await commands.listMeetings({ limit: 10, offset: 0 });  // typed as Meeting[]
```

This **catches type mismatches at compile time** (the main reason this
file exists) even before `tauri-specta` is wired up.

## Next step: PR #1.5 (tauri-specta integration)

PR #1.5 will:

1. Add `specta`, `specta-typescript`, `tauri-specta` to
   `crates/qmeetily-app/Cargo.toml`.
2. Annotate every `#[tauri::command]` with `#[specta::specta]`.
3. Add a builder in `src/lib.rs` that exports to `frontend/src/lib/bindings.ts`
   on debug builds.
4. Delete this placeholder file — the generated one will replace it.

## Why not just hand-write types forever?

Hand-written types drift. Within weeks, `commands.getMeeting` will return
`Meeting | null` here but `Meeting` in Rust. The whole point of
`tauri-specta` is to make the binding live and machine-checked.

## Compatibility with the parent meetily frontend

The parent `analysis_outputs/frontend/` has its own transport layer at
`src/lib/transport.ts`. QMeetily's frontend at
`analysis_outputs/QMEETILY/frontend/` is fully independent — they share
no runtime code.
