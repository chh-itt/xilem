//! SignalScope — pairs a [`TaskScope`] with signal lifecycle management.
//!
//! `SignalScope` is the bridge between Auralis's reactive signals and
//! Xilem's View-based component model.  It owns a [`TaskScope`] for
//! async side effects and provides conveniences for:
//!
//! - `watch()` — spawn a task that re-runs on every signal change
//! - `watch_effect()` — auto-tracking effect (uses Memo internally)
//! - `provide_signal()` / `consume_signal()` — type-erased signal sharing
//!   across component boundaries via the scope context
//!
//! The scope is automatically cancelled when the owning component's
//! view is torn down (via [`Drop`]), cleaning up all spawned tasks and
//! signal subscriptions.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use auralis_signal::Signal;
use auralis_task::{CallbackHandle, JoinHandle, TaskScope};

/// A signal-aware scope that owns async tasks and manages signal
/// subscriptions for the lifetime of a component.
///
/// # Lifecycle
///
/// Create one in a component's `build()` path, register callbacks on
/// the component's scope, and return it alongside the View.  When the
/// scope is dropped (component unmounts), all tasks are cancelled
/// and all signal subscriptions are removed.
///
/// # Example
///
/// ```ignore
/// use xilem_core_auralis::SignalScope;
///
/// let scope = SignalScope::new();
/// scope.watch(&count, |val| {
///     println!("count changed to {val}");
/// });
/// // returns `scope` — will be dropped when component unmounts
/// ```
#[must_use]
pub struct SignalScope {
    inner: TaskScope,
    /// Type-erased signal storage for cross-component sharing via
    /// `provide_signal` / `consume_signal`.
    signals: Rc<RefCell<HashMap<TypeId, Box<dyn std::any::Any>>>>,
}

impl SignalScope {
    /// Create a new signal scope backed by a fresh [`TaskScope`].
    pub fn new() -> Self {
        Self {
            inner: TaskScope::new(),
            signals: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Create a child scope (for sub-components).
    pub fn new_child(parent: &Self) -> Self {
        Self {
            inner: TaskScope::new_child(&parent.inner),
            signals: Rc::clone(&parent.signals),
        }
    }

    /// Spawn a task that calls `f` whenever `sig` changes.
    ///
    /// The task runs `f(&new_value)` after each `Signal::set()`.  It is
    /// automatically cancelled when this scope is dropped.
    pub fn watch<T: Clone + 'static>(
        &self,
        sig: &Signal<T>,
        mut f: impl FnMut(&T) + 'static,
    ) -> JoinHandle {
        let s = sig.clone();
        self.inner.spawn(async move {
            loop {
                s.changed().await;
                f(&s.read());
            }
        })
    }

    /// Spawn an auto-tracking effect.
    ///
    /// The `effect` closure is called once immediately to discover its
    /// signal dependencies (via a [`Memo`]).  Subsequent calls happen
    /// automatically when any dependency changes.
    pub fn watch_effect(&self, effect: impl Fn() + 'static) -> JoinHandle {
        self.inner.watch_effect(effect)
    }

    /// Register a cleanup function that runs when this scope is dropped.
    pub fn on_cleanup(&self, f: impl FnOnce() + 'static) {
        self.inner.on_cleanup(f);
    }

    /// Register a callback handle for cleanup.
    pub fn register_callback(&self, handle: CallbackHandle) {
        self.inner.register_callback_handle(handle);
    }

    /// Return a reference to the inner [`TaskScope`].
    pub fn task_scope(&self) -> &TaskScope {
        &self.inner
    }

    /// Store a value in this scope's type-erased signal map.
    ///
    /// Subsequent calls to [`consume_signal`](Self::consume_signal) on
    /// this scope (or any child) will find it.
    ///
    /// Unlike [`TaskScope::provide`], this is specifically for sharing
    /// `Signal<T>` instances across component boundaries without needing
    /// to thread them through every function signature.
    pub fn provide_signal<T: 'static>(&self, signal: Signal<T>) {
        self.signals
            .borrow_mut()
            .insert(TypeId::of::<T>(), Box::new(signal));
    }

    /// Look up a signal of type `T` from this scope's signal map.
    ///
    /// Returns `None` if no signal of this type has been provided.
    pub fn consume_signal<T: 'static>(&self) -> Option<Signal<T>> {
        self.signals
            .borrow()
            .get(&TypeId::of::<T>())
            .and_then(|any| any.downcast_ref::<Signal<T>>())
            .cloned()
    }

    /// Suspend all tasks in this scope (and children).
    ///
    /// Suspended tasks are skipped during executor polling until
    /// [`resume`](Self::resume) is called.
    pub fn suspend(&self) {
        self.inner.suspend();
    }

    /// Resume all tasks in this scope (and children).
    pub fn resume(&self) {
        self.inner.resume();
    }

    /// Return `true` if this scope is currently suspended.
    pub fn is_suspended(&self) -> bool {
        self.inner.is_suspended()
    }
}

impl std::fmt::Debug for SignalScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalScope")
            .field("cancelled", &self.inner.is_cancelled())
            .field("suspended", &self.inner.is_suspended())
            .finish_non_exhaustive()
    }
}

impl Clone for SignalScope {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            signals: Rc::clone(&self.signals),
        }
    }
}

impl Default for SignalScope {
    fn default() -> Self {
        Self::new()
    }
}
