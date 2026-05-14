//! # xilem_core_auralis
//!
//! Signal-based reactive views that implement `xilem_core::View`.
//!
//! This crate replaces xilem_core's `PartialEq`-based `Memoize` with
//! Auralis `Signal<T>` version-tracking: O(1) change detection, no
//! `PartialEq` bound, auto-increment on `set()`.
//!
//! ## What's different from xilem_core
//!
//! | xilem_core | xilem_core_auralis |
//! |---|---|
//! | `memoize(data, \|d\| view)` requires `Data: PartialEq` | `signal_memoize(sig, \|s\| view)` uses `Signal::version()` |
//! | Equality check O(n) per rebuild | Version check O(1) |
//! | No built-in async support | `TaskScope` for async tasks |
//! | Manual dirty flagging | Auto-dirty via signal callbacks |
//!
//! ## Quick example
//!
//! ```ignore
//! use auralis_signal::Signal;
//! use xilem_core_auralis::{signal_memoize, watch_signal};
//! use xilem_masonry::view::{label, text_button, flex_row};
//!
//! let count = Signal::new(0);
//! signal_memoize(count.clone(), |count| {
//!     flex_row((
//!         label(format!("count: {}", count.read())),
//!         text_button("+", move |_: &mut ()| { count.set(count.read() + 1); }),
//!     ))
//! })
//! ```

mod signal_memoize;
mod watch_signal;
mod effect_view;
mod state_memoize;
mod signal_state_memoize;

pub use signal_memoize::{signal_memoize, SignalMemoize};
pub use watch_signal::{watch_signal, WatchSignal};
pub use effect_view::{effect_view, EffectView};
pub use state_memoize::{state_memoize, StateMemoize};
pub use signal_state_memoize::{signal_state_memoize, SignalStateMemoize};

// Re-export xilem_core traits for convenience
pub use xilem_core::{
    MessageCtx, MessageResult, Mut, View, ViewElement, ViewMarker, ViewPathTracker,
};
