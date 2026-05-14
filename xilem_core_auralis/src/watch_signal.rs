use core::fmt::Debug;
use core::marker::PhantomData;

use auralis_signal::Signal;
use xilem_core::{MessageCtx, MessageResult, Mut, View, ViewMarker, ViewPathTracker};

/// Wraps a view and marks it dirty whenever `signal` changes.
///
/// Unlike [`SignalMemoize`](crate::SignalMemoize), this does not re-run a
/// view constructor — it just forces a `rebuild()` on the inner view when
/// the signal version increments.  Useful when the inner view already
/// knows how to read the current state but needs to be told *when* to
/// re-render.
///
/// # Example
///
/// ```ignore
/// use auralis_signal::Signal;
/// use xilem_core_auralis::watch_signal;
///
/// let data = Signal::new("hello");
/// let view = watch_signal(data, label("hello"));
/// // When data.set("world") is called, the label will be rebuilt.
/// ```
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct WatchSignal<V, Data, State, Action, Context> {
    signal: Signal<Data>,
    inner: V,
    phantom: PhantomData<fn() -> (State, Action, Context)>,
}

impl<V: Debug, Data, State, Action, Context> Debug
    for WatchSignal<V, Data, State, Action, Context>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WatchSignal")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

#[expect(
    unnameable_types,
    reason = "Implementation detail, public because of trait visibility rules"
)]
#[allow(missing_debug_implementations)]
pub struct WatchSignalState<VState> {
    inner_state: VState,
    last_version: u64,
    dirty: bool,
}

/// Create a [`WatchSignal`] view.
pub fn watch_signal<Data, V, State, Action, Context>(
    signal: Signal<Data>,
    inner: V,
) -> WatchSignal<V, Data, State, Action, Context>
where
    State: 'static,
    Data: 'static,
    V: View<State, Action, Context>,
    Context: ViewPathTracker,
{
    WatchSignal {
        signal,
        inner,
        phantom: PhantomData,
    }
}

impl<V, Data, State, Action, Context> ViewMarker
    for WatchSignal<V, Data, State, Action, Context>
{
}

impl<Data, V, State, Action, Context> View<State, Action, Context>
    for WatchSignal<V, Data, State, Action, Context>
where
    State: 'static,
    Action: 'static,
    Context: ViewPathTracker + 'static,
    Data: 'static,
    V: View<State, Action, Context>,
{
    type Element = V::Element;
    type ViewState = WatchSignalState<V::ViewState>;

    fn build(
        &self,
        ctx: &mut Context,
        app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let (element, inner_state) = self.inner.build(ctx, app_state);
        let state = WatchSignalState {
            inner_state,
            last_version: self.signal.version(),
            dirty: false,
        };
        (element, state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if !Signal::ptr_eq(&self.signal, &prev.signal) {
            view_state.last_version = 0;
        }

        let current_version = self.signal.version();
        if core::mem::take(&mut view_state.dirty) || view_state.last_version != current_version {
            self.inner.rebuild(
                &prev.inner,
                &mut view_state.inner_state,
                ctx,
                element,
                app_state,
            );
            view_state.last_version = current_version;
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
    ) {
        self.inner
            .teardown(&mut view_state.inner_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let result = self.inner.message(
            &mut view_state.inner_state,
            message,
            element,
            app_state,
        );
        if matches!(result, MessageResult::RequestRebuild) {
            view_state.dirty = true;
        }
        result
    }
}
