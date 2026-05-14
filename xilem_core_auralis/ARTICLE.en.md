# Replacing Xilem's Reactive Layer with Auralis: An Experiment

## Before We Begin

First, I've been following Xilem for a while and have deep respect for its architecture — the clean layering, the Masonry widget foundation, the Elm-inspired message routing. It's one of the most thoughtfully designed Rust GUI projects.

I'm building [Auralis](https://github.com/chh-itt/auralis), a reactive kernel based on push signals (`Signal<T>` + `Memo<T>`). I got curious: *what would happen if I plugged push-based signals into Xilem's View layer, replacing the polling-based `memoize`?* So I ran an experiment.

**This is not a PR, not a proposal, not "this should be changed."** It's a data point for the community. Maybe someone else is thinking about similar tradeoffs. The code is in [this fork](https://github.com/chh-itt/xilem) under `xilem_core_auralis/`.

---

## The Two Reactive Models

### Xilem: Polling Reconciliation

```
Every frame → app_logic() → rebuild View tree → memoize checks prev ≠ self → diff → update widget
```

- Every item is checked every frame, regardless of whether it changed.
- Uses `Data: PartialEq` to detect changes.
- Views are ephemeral, recreated each frame.

### Auralis: Push Signals

```
signal.set() → version increments → subscribers notified → Memo recomputes → effect fires
```

- Changes propagate only when they happen, targeting exactly the right signal.
- Uses O(1) version comparison — no `PartialEq` needed.
- Signals are persistent across frames.

### Feature Comparison Across Paradigms

Different paradigms naturally lead to different feature sets. A polling model doesn't need subscriptions; a push model doesn't need per-frame diffing. Neither is "missing" anything — they just solve the problem differently.

|  | xilem_core | auralis-signal |
|------|-----------|---------------|
| Change detection | `PartialEq` per frame | Signal version (O(1)) |
| Auto dependency tracking | — | `Memo<T>` |
| Async task management | — (external: Tokio, etc.) | `TaskScope` |
| Side-effect management | — (manual) | `watch_effect` |
| Cooperative timer | — (external) | `timer::sleep` |
| Batch updates | — | `batch()` |

"—" means the paradigm doesn't require it, not that it's "missing."

---

## Code Size: 3.1× Smaller

| | auralis-signal | xilem_core |
|------|---------------|------------|
| Implementation | **1,779 lines** | 5,467 lines |
| Tests | 1,832 lines | 2,056 lines |
| **Total** | **3,611 lines** | **7,523 lines** |

`auralis-signal` fits in 6 source files. The entire reactive layer can be read in one sitting.

---

## Compile Time & Dependencies: Zero vs. Six

| | auralis-signal | xilem_core |
|------|---------------|------------|
| Direct dependencies | **0** | 4 |
| Transitive dependencies | **0** | 6 |
| Dependency tree depth | 1 | 3 |
| Clean build (dev) | **0.30s** | 1.09s |

```
auralis-signal:            xilem_core:
    (nothing)                  ├── anymore
                               ├── hashbrown
                               │   └── foldhash
                               └── tracing
                                   ├── pin-project-lite
                                   └── tracing-core
```

---

## Performance: Raw Comparison Cost

The only difference in `View::rebuild` between the two approaches is the "has it changed?" check. Everything else (trait dispatch, context passing, inner view rebuild) is identical.

### On the Fairness of This Benchmark

PartialEq on identical large strings represents the worst case for polling. However, in GUI applications, the vast majority of frames have no changes at all — making the worst case also the most common path. For data that *has* changed, PartialEq short-circuits on the first differing element, narrowing the gap considerably. The O(1) version check still holds an advantage, but the practical takeaway is not about raw speed — it's about eliminating the `PartialEq` constraint entirely.

```
2M iterations, release build, data unchanged (the typical frame path)

Data type            | memoize (PartialEq) | state_memoize (u64) | Ratio
---------------------|--------------------|--------------------|------
i32                  |   993µs           |   971µs           |   1×
String 16B           |  4.02ms           |   983µs           |   4×
String 64B           |  7.90ms           |   989µs           |   8×
String 1KB           |  37.1ms           |   883µs           |  42×
String 12KB          |   442ms           |  1.49ms           | 297×
Vec<String> 100 items |   447ms           |   989µs           | 452×
Vec<String> 1000 items | **5.54s**        |   969µs           | **5713×**
```

The important caveat: in real GUI applications, widget rendering dominates frame time. The comparison cost difference is rarely the bottleneck. The value of O(1) version checking is not raw FPS — it's the elimination of the `PartialEq` constraint and the architectural simplicity it enables.

**Run:** `cargo run --example bench -p xilem_core_auralis --release`

---

## Memory: Zero Per-Frame Allocation vs. Per-Frame Clone

| Approach | View Size | Data Location | Per-frame Allocation |
|----------|----------|---------------|---------------------|
| `memoize<i32>` | 4 B | Inline in View | 0 |
| `memoize<String>` | 24 B | Inline in View | **Clone String (heap)** |
| `state_memoize` | 0–16 B | AppState (persistent) | 0 |
| `signal_state_memoize` | 0 B | Signal in AppState | 0 |

The real difference is not total memory — it's allocation pressure per frame:

```
1,000 String items × 60 fps:

memoize:      1,000 String::clone() calls per frame → 60,000 heap allocations per second
state_memoize: 0 allocations per frame (data lives in AppState)
signal_state:  0 allocations per frame + automatic version tracking
```

Non-capturing closures are ZSTs (zero-sized types) — Rust eliminates them entirely at compile time.

**Run:** `cargo run --example bench_memory -p xilem_core_auralis --release`

---

## Design Tradeoffs Discovered

### Signal<T> is !Send + !Sync — and AppState saved us

`Signal<T>` uses `Rc<RefCell<>>` internally — by design, it's single-threaded. Xilem's `WidgetView` requires `View + Send + Sync`.

The workaround turned out to be surprisingly clean: store the `Signal` in `AppState` and use function pointers in the View to access it. `State` in `View<State, Action, Context>` only requires `'static` — no `Send` or `Sync`. Function pointers, meanwhile, are always `Send + Sync`.

```rust
// Signal lives in AppState: !Send + !Sync is fine here
struct AppState {
    count: Signal<i32>,
}

// View stores only fn pointers: always Send + Sync
signal_state_memoize(
    |s: &AppState| s.count.clone(),   // fn pointer
    |count: &Signal<i32>| { ... },    // fn pointer
)
```

This constraint also turned out to be a useful forcing function: it pushed us toward a cleaner separation of "persistent reactive state" (AppState) from "transient view descriptions" (the View tree). The right design emerged from respecting the constraint, not fighting it.

---

## Compatibility: All Existing Tests Pass

| Test Suite | Passed | Failed |
|-----------|--------|--------|
| xilem_core (52 tests) | **52** | 0 |
| xilem_core_auralis (8 tests) | **8** | 0 |

Notably, xilem_core has no tests for `memoize` behavior. Our `tests/comparison.rs` provides the only memoize coverage in the ecosystem.

---

## API Comparison

### xilem_core::memoize (original)

```rust
memoize(state.count, |count| {
    label(format!("Count: {count}"))
})
// Requires Data: PartialEq
// Data cloned from AppState into Memoize struct every frame
```

### Auralis state_memoize (manual version tracking)

```rust
state_memoize(
    |s: &AppState| s.count,
    |s: &AppState| s.version.get(),  // Cell<u64>
    |count: &i32| label(format!("Count: {count}")),
)
// No PartialEq needed
// Data lives in AppState, View stores only fn pointers
```

### Auralis signal_state_memoize (automatic tracking) ★

```rust
struct AppState {
    count: Signal<i32>,  // auto-incrementing version
}

signal_state_memoize(
    |s: &AppState| s.count.clone(),
    |count: &Signal<i32>| label(format!("Count: {}", count.read())),
)
// Signal::set() → version auto-increments
// Zero manual bookkeeping
```

---

## GUI Demo

A real Masonry widget comparison running both approaches side by side:

```
cargo run --example live_comparison -p xilem_core_auralis --release
```

---

## Comparison Table

⚠️ **This is not a competition scoreboard. It's a map of engineering tradeoffs between two reactive paradigms in the same GUI scenario.**

| Dimension | memoize | state_memoize | signal_state_memoize |
|-----------|---------|---------------|---------------------|
| Detection method | PartialEq | u64 version | u64 version (auto) |
| Requires PartialEq | ✅ required | ❌ | ❌ |
| Check complexity | O(n) | O(1) | O(1) |
| WidgetView compatible | ✅ | ✅ | ✅ |
| Manual bookkeeping | None | Cell\<u64\> | ✅ automatic |
| View size (i32) | 4 B | 0–16 B | 0 B |
| View size (String) | 24 B + heap | 0–16 B | 0 B |
| Per-frame allocation | Clone data | 0 | 0 |
| Raw comparison (12KB) | 442ms | 1.5ms (297×) | 1.5ms (297×) |
| Auto dependency tracking | — | — | ✅ Memo\<T\> |
| Async support | — | — | ✅ TaskScope |
| Batch updates | — | — | ✅ batch() |

"—" means the paradigm doesn't require it, not that it's "missing."

---

## What This Is Not

- **Not a critique of Xilem's design.** Polling reconciliation is a perfectly valid approach for GUI applications. Rendering dominates frame time, and PartialEq on typical UI data (integers, short strings) is essentially free.
- **Not a PR or improvement proposal.** This is an experiment in swapping reactive primitives. The results are shared as a data point, not a prescription.
- **Not about "winning."** The value is in understanding how different reactive models shape engineering decisions — what becomes possible, what becomes necessary, what becomes irrelevant.

---

## Limitations of This Experiment

- **Scope.** We replaced only the `memoize`/`Frozen` layer — the change-detection mechanism inside `View::rebuild`. We did not attempt to replace the `View` trait itself, the `ViewSequence` infrastructure, or the message routing system. A full signal-based UI framework would look quite different from Xilem, and this experiment doesn't explore that.

- **Memoize is evolving.** Xilem's own documentation notes: *"The story of Memoization in Xilem is still being worked out, so the details of this view might change."* Our comparison is against the current implementation (Xilem 0.4.0), not against any future design direction. The Xilem team may already be considering approaches that address some of the tradeoffs discussed here.

- **Microbenchmarks, not applications.** The performance data comes from isolated rebuild loops. In a real application, widget rendering, layout, and GPU work dominate frame time. The comparison cost difference is real but may not be the primary factor in most GUI scenarios. The architectural differences (no `PartialEq`, zero per-frame allocation) are likely more impactful than the raw numbers.

- **Single-threaded assumption.** Auralis signals are intentionally single-threaded (`!Send + !Sync`). This experiment works because Xilem's `State` has no threading constraint, but a production integration might need multi-threaded signal variants (e.g., `Arc<Mutex<>>`-backed) for use cases like multi-window or SSR isolation.

- **No long-running application data.** We tested frame-level rebuild behavior, not application-level concerns like startup time, hot-reload compatibility, or memory usage over hours of runtime. These matter for real-world adoption.

- **Type erasure cost.** Our 1000-label experiment used `.boxed()` for type erasure (to produce homogeneous `Vec` types). This adds a layer of dynamic dispatch not present in typical Xilem usage patterns. The impact is small but not zero.

---

## Reproducing

```bash
git clone https://github.com/linebender/xilem.git
# Copy xilem_core_auralis/ into the xilem workspace

# Comparison benchmarks
cargo run --example bench -p xilem_core_auralis --release
cargo run --example bench_memory -p xilem_core_auralis --release

# GUI demo
cargo run --example live_comparison -p xilem_core_auralis --release

# Tests
cargo test -p xilem_core_auralis   # 8 passed
cargo test -p xilem_core            # 52 passed (nothing broken)
```

---

## What I'd Love Feedback On

- Are there other dimensions worth measuring? (binary size, incremental rebuild, `virtual_scroll` integration)
- Has anyone else experimented with push-based reactivity in Xilem or similar GUI frameworks?
- What tradeoffs matter most to you when choosing a reactive model for GUI applications?
- The `WidgetView` trait's `Send + Sync` requirement was one of the more interesting constraints we ran into. We worked around it, but we'd love to hear if there are use cases where relaxing this constraint might open up new design possibilities.
