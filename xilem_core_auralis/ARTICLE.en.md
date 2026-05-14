# Three Ways to Detect Change in Xilem: A Comparative Experiment

I wanted to understand how different reactive models play out inside the same GUI framework. So I built `xilem_core_auralis` — a crate that adds push-signal change detection to Xilem's `View::rebuild` path — and ran the numbers. This is what I found.

The code is in [this fork](https://github.com/chh-itt/xilem/tree/auralis-experiment) under `xilem_core_auralis/`.

---

## 1. How Xilem Handles Change Detection Today

Xilem's view tree is event-driven: `app_logic` runs when a user action changes state, and `rebuild()` compares the new View tree against the previous one. For deciding *which parts of the tree actually need updating*, Xilem provides three strategies:

| Strategy | How it detects change | Complexity | Who does the work? |
|----------|----------------------|------------|-------------------|
| `memoize(data, fn)` | `Data: PartialEq` | O(n) | Framework |
| `Arc<impl View>` | `Arc::ptr_eq` | O(1) | **Developer** |
| `frozen(fn)` | Nothing (dirty flag only) | O(1) | Framework |

`frozen` is for views that never depend on external state — build once, never rebuild unless a child requests it. It's the simplest case and not our focus here.

The interesting contrast is between the two general-purpose strategies:

**`memoize`** is automatic. The framework compares the old and new data each time `rebuild` runs. The cost is `Data: PartialEq` — trivial for integers, acceptable for short strings, painful for large collections.

**`Arc<impl View>`** is fast. Comparison is a single `Arc::ptr_eq` — one pointer comparison. But the developer carries the burden: cache the `Arc` in `AppState`, manually decide when to create a new one, clone or replace as needed.

```rust
// Arc<impl View> — developer manages the lifecycle:
fn increase_button(state: &mut AppState) -> Arc<AnyWidgetView<AppState>> {
    if state.count == state.cached_count   // ← developer writes this
        && let Some(view) = &state.cached_view
    {
        view.clone()                       // same Arc → ptr_eq → skip
    } else {
        let view = Arc::new(text_button(...));
        state.cached_count = state.count;  // ← developer writes this
        state.cached_view = Some(view.clone());
        view                               // new Arc → rebuild
    }
}
```

---

## 2. What a Push-Signal Model Adds

Auralis's approach is to give each piece of data a monotonic version number. `Signal::set()` increments it. The framework compares versions instead of values. This sits between the two existing strategies: automatic like `memoize`, O(1) like `Arc<impl View>`.

I built two variants:

```rust
// state_memoize — version managed manually, via Cell<u64>:
state_memoize(
    |s: &AppState| s.count,
    |s: &AppState| s.version.get(),
    |count: &i32| label(format!("Count: {count}")),
)
// User bumps version: s.version.set(s.version.get() + 1)

// signal_state_memoize — version managed automatically, via Signal<T>:
struct AppState { count: Signal<i32> }

signal_state_memoize(
    |s: &AppState| s.count.clone(),
    |count: &Signal<i32>| label(format!("Count: {}", count.read())),
)
// Signal::set() bumps version automatically
```

Both are `Send + Sync` compatible — the `Signal` lives in `AppState` (which has no threading constraint), and the View stores only function pointers.

---

## 3. The Core Tradeoff: Who Decides "Changed"?

The raw comparison costs set the stage:

```
2M checks, release build

Strategy                              | Per check | Complexity
--------------------------------------|-----------|-----------
Arc::ptr_eq (same Arc)                |   1.3ns   | O(1)
Signal::version (unchanged)           |   1.1ns   | O(1)
memoize PartialEq — i32               |   0.5ns   | O(1)
memoize PartialEq — String 16B        |   2.0ns   | O(n)
memoize PartialEq — String 12KB       |   221ns   | O(n)
memoize PartialEq — Vec<String> 1000  |  2770ns   | O(n)
```

For small `Copy` types like `i32`, all three strategies are equivalent — `PartialEq` on an integer is just one integer comparison. The 0.5ns vs 1.3ns difference is noise; both are ~1ns. The gap only opens when data grows beyond a single register: strings, vectors, collections.

`Arc::ptr_eq` and `Signal::version()` stay at one integer comparison regardless of data size. `memoize`'s `PartialEq` scales with the data. The difference is not in the instruction count. It's in the API surface:

| | memoize | Arc\<impl View\> | signal_state_memoize |
|------|---------|---------------------|---------------------|
| Detection | `PartialEq` (O(n)) | `Arc::ptr_eq` (O(1)) | `version()` (O(1)) |
| Who decides? | Framework | Developer | Framework |
| Developer writes... | Nothing | Manual comparison + cache logic | Nothing |
| PartialEq required | Yes | No | No |
| Per-rebuild allocation | Clone data | `Arc::clone` (refcount) | 0 |

`memoize` gives you automation at the cost of `PartialEq`. `Arc<impl View>` gives you speed at the cost of manual bookkeeping. `signal_state_memoize` gives you automation and speed — the framework handles version tracking, the developer just calls `signal.set()`.

---

## 4. The "Smaller" Side of the Story

Code size, dependencies, and compile time — the engineering overhead of each reactive layer:

| | auralis-signal | xilem_core |
|------|---------------|------------|
| Implementation | **1,779 lines** | 5,467 lines |
| Dependencies | **0** | 4 (6 transitive) |
| Clean build | **0.30s** | 1.09s |

`auralis-signal` is 6 files. Zero dependencies means zero dependency conflicts, zero supply-chain surface, and instant compilation in any project.

```
auralis-signal:            xilem_core:
    (nothing)                  ├── anymore
                               ├── hashbrown → foldhash
                               └── tracing  → tracing-core → pin-project-lite
```

---

## 5. Memory: Where Does the Data Live?

| Approach | View struct size | Data location | Allocation per rebuild |
|----------|-----------------|---------------|----------------------|
| `memoize<i32>` | 4 B | Inline | 0 |
| `memoize<String>` | 24 B | Inline | Clone String (heap) |
| `Arc<impl View>` | 8 B | Arc in AppState | Arc::clone (refcount) |
| `state_memoize` | 0–16 B | AppState | 0 |
| `signal_state_memoize` | 0 B | Signal in AppState | 0 |

The pattern is consistent: `memoize` copies data into the View struct; the other strategies reference persistent data in AppState. Non-capturing closures are zero-sized — Rust eliminates them at compile time.

---

## 6. A Constraint That Became a Feature

`Signal<T>` is `!Send + !Sync` by design — it uses `Rc<RefCell<>>` internally. Xilem's `WidgetView` requires `Send + Sync`.

The fix: put the `Signal` in `AppState`. `State` in `View<State, Action, Context>` only requires `'static` — no threading bounds. The View stores function pointers (always `Send + Sync`) that extract the Signal during `build`/`rebuild`.

This constraint forced a clean separation: persistent reactive state lives in AppState; transient view descriptions live in the View tree. The design fell out naturally from respecting the boundary rather than fighting it.

---

## 7. Compatibility

All 52 `xilem_core` tests pass unchanged. We added 8 tests for `memoize`/`signal_memoize` behavior — xilem_core currently has no memoize-specific tests.

```
cargo test -p xilem_core          # 52 passed
cargo test -p xilem_core_auralis  # 8 passed
```

---

## 8. Limitations

- **Scope.** We modified only the change-detection layer inside `View::rebuild`. We did not touch the `View` trait, `ViewSequence`, or message routing. A full signal-based UI framework would differ more fundamentally.
- **Memoize is evolving.** Xilem's docs note: *"The story of Memoization in Xilem is still being worked out, so the details of this view might change."* This is a snapshot against 0.4.0.
- **`Arc<impl View>` comparison is at the micro level.** We measured `Arc::ptr_eq` directly but did not implement a full head-to-head benchmark exercising the manual-caching pattern inside `app_logic`.
- **Microbenchmarks, not applications.** Rendering and layout dominate real GUI frame times. The architectural differences matter more than the nanosecond-level comparison costs.
- **Single-threaded.** Auralis signals are `!Send + !Sync`. Production multi-window or SSR use would need multi-threaded variants.

---

## 9. Reproducing

```bash
git clone https://github.com/chh-itt/xilem.git
cd xilem
git checkout auralis-experiment

cargo run --example bench -p xilem_core_auralis --release        # PartialEq vs version
cargo run --example bench_arc -p xilem_core_auralis --release     # Arc vs Signal vs PartialEq
cargo run --example bench_memory -p xilem_core_auralis --release  # Memory footprint
cargo run --example live_comparison -p xilem_core_auralis --release  # GUI demo
```

---

## 10. Open Questions

- Is `Arc<impl View>` + manual caching the intended primary path for O(1) memoization, or is automated dependency tracking on the roadmap?
- Has anyone explored subscription-based approaches within Xilem's event-driven model?
- The `WidgetView::Send + Sync` constraint — we found a clean workaround, but we're curious: are there other situations where this constraint shapes design in interesting ways?
- Other dimensions worth measuring?
