# Auralis Signals in Xilem — A Fair Comparison

This document replaces the earlier experiment write-up on Zulip.  It reflects
what we learned after building a full signal-driven component demo inside the
Xilem/Masonry widget framework.

## What was built

- `xilem_core_auralis` — 5 signal-based `View` adapters that implement
  `xilem_core::View`, plus a `SignalScope` helper for async task lifecycle.
- `examples/todomvc_auralis.rs` — side-by-side demo: left panel uses
  `xilem_core::memoize`, right panel uses `auralis::signal_state_memoize`.
  A third panel demonstrates cross-component signal sharing.
- Click "Print DevTools Snapshot" to dump the full reactive graph
  (signals, memos, derivation tree) to stdout.

## Architecture

```
┌─────────────────────────────────────────────┐
│  Xilem / Masonry (framework)                │
│  WidgetView trait, flex, text_button, etc.  │
├─────────────────────────────────────────────┤
│  xilem_core (View trait infrastructure)     │
│  View, ViewSequence, MessageCtx, Element    │
├──────────────────┬──────────────────────────┤
│  xilem_core      │  xilem_core_auralis      │
│  ─────────       │  ─────────────────       │
│  memoize         │  signal_state_memoize    │
│  lens            │  signal_memoize          │
│  map_state       │  effect_view             │
│  frozen          │  watch_signal            │
│  PartialEq-based │  Signal::version()-based │
├──────────────────┴──────────────────────────┤
│  auralis-signal (reactive kernel)           │
│  Signal<T>, Memo<T>, batch, observer        │
└─────────────────────────────────────────────┘
```

xilem_core_auralis does NOT replace xilem_core.  It depends on xilem_core for
the `View` trait, `ViewSequence`, `MessageCtx`, and all widget infrastructure.
It provides an *alternative* change-detection layer.

## Code size (honest numbers)

| Layer | xilem_core | auralis |
|---|---|---|
| Framework infrastructure (View trait, etc.) | 3,796 lines | reuses xilem_core |
| State management layer (memoize + lens + …) | **675 lines** (pure code) | — |
| Signal adapter layer (5 View adapters) | — | **720 lines** (pure code) |
| Reactive kernel (Signal, Memo, batch, …) | — | **1,296 lines** (pure code) |
| **State management total** | **675 lines** | **2,016 lines** |

Line counts are pure Rust code (no comments, no blanks, no tests) via `tokei`.

**The "3× smaller" claim from the earlier experiment is retracted.**  It only
measured replacing `memoize` without counting the Auralis kernel itself.
Auralis is ~3× *more* code for the same functional scope — this is an upgrade
path with additional capabilities, not a lightweight replacement.

## Writing components

### Simple counter

**xilem_core:**
```rust
fn counter(state: &AppState) -> impl WidgetView<AppState> {
    memoize(state.count, |c| label(format!("{c}")))
}
```

**auralis:**
```rust
fn counter(_state: &AppState) -> impl WidgetView<AppState> {
    signal_state_memoize(
        |s: &AppState| s.count.clone(),
        |c: &Signal<i32>| label(format!("{}", c.read())),
    )
}
```

**Verdict:** For simple components, memoize is shorter and more direct.  The
two-closure syntax in `signal_state_memoize` is structural — `Signal<T>` is
`!Send + !Sync`, so it cannot be stored in `WidgetView` (`Send + Sync` required).
The fn-pointer extraction from `&State` is the adapter cost, not an API flaw.

This cost exists at every framework boundary.  The "native" Auralis experience
(no View trait) would be:

```rust
let view = Memo::new(move || {
    label(format!("{}", count.read()))
});
```

Zero closures — `Memo` auto-tracks.  The closures appear only when bridging
back into a View trait framework.

### Cross-component state sharing

In xilem_core, sharing state between sibling components requires `lens` to
project sub-state or passing `&mut AppState` through the tree.

In auralis, the same `Signal<T>` can be cloned and read by any component.
When one calls `set()`, all others see the version bump on the next frame:

```rust
// Both components read the SAME signal
fn component_a(_state: &AppState) -> impl WidgetView<AppState> {
    signal_state_memoize(|s| s.count.clone(), |c| label(format!("A: {}", c.read())))
}
fn component_b(_state: &AppState) -> impl WidgetView<AppState> {
    signal_state_memoize(|s| s.count.clone(), |c| label(format!("B: {}", c.read())))
}
```

No lens, no message wiring.

## Where each approach wins

| | xilem_core | auralis |
|---|---|---|
| Simple components | **shorter, more natural** | two-closure overhead |
| Code size | **675 lines** | 2,016 lines |
| Maintenance complexity | **lower** (single crate, no_std) | higher (multi-crate, std) |
| No `PartialEq` bound | ❌ | ✅ |
| Cross-context state sharing | limited to `&mut State` | `Signal::clone()` works anywhere |
| Runtime diagnostics | none | `snapshot()`, `diff_snapshots()`, derivation tree |
| Hot-reload potential | low (state in ViewState) | **high** (signal decoupled from component) |

## Conclusion

Auralis is not a "better xilem_core."  It is an alternative change-detection
layer that trades simplicity for capability.  For most apps, `memoize` +
`PartialEq` is the right default.  When you hit its limits — types without
`PartialEq`, cross-cutting state that lens can't express cleanly, or a need
for runtime observability — auralis is there.
