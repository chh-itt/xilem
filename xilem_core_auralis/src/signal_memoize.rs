use core::fmt::Debug;
use core::marker::PhantomData;

use auralis_signal::Signal;
use xilem_core::{MessageCtx, MessageResult, Mut, View, ViewMarker, ViewPathTracker};

/// A view that memoizes using `Signal::version()` instead of `Data: PartialEq`.
///
/// # Differences from `xilem_core::memoize`
///
/// - **No `PartialEq` bound** — O(1) version comparison replaces O(n) equality.
/// - **Signal-based** — the caller owns the `Signal<Data>`, mutations
///   through `signal.set()` auto-increment the version.
/// - **Same signal, different frames** — as long as the same `Signal`
///   (by `Rc` identity) is passed to consecutive frames, version
///   comparison is sufficient.  If the signal was replaced, `ptr_eq`
///   detects that and forces a rebuild.
///
/// # Example
///
/// ```ignore
/// use auralis_signal::Signal;
/// use xilem_core_auralis::signal_memoize;
///
/// let count = Signal::new(0);
/// let view = signal_memoize(count, |count| {
///     // build view that reads `count`
/// });
/// ```
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SignalMemoize<V, ViewFn, Data, State, Action, Context> {
    signal: Signal<Data>,
    view_fn: ViewFn,
    phantom: PhantomData<fn() -> (V, State, Action, Context)>,
}

impl<V, ViewFn, Data, State, Action, Context> Debug
    for SignalMemoize<V, ViewFn, Data, State, Action, Context>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SignalMemoize")
            .field("version", &self.signal.version())
            .finish_non_exhaustive()
    }
}

/// Create a [`SignalMemoize`] view.
///
/// The `view_fn` receives a `&Signal<Data>` and returns a view.  When the
/// signal's version changes, the view is rebuilt.  Unlike `xilem_core::memoize`,
/// this does not require `Data: PartialEq`.
pub fn signal_memoize<Data, V, ViewFn, State, Action, Context>(
    signal: Signal<Data>,
    view_fn: ViewFn,
) -> SignalMemoize<V, ViewFn, Data, State, Action, Context>
where
    State: 'static,
    Data: 'static,
    ViewFn: Fn(&Signal<Data>) -> V + 'static,
    V: View<State, Action, Context>,
    Context: ViewPathTracker,
{
    SignalMemoize {
        signal,
        view_fn,
        phantom: PhantomData,
    }
}

/// Internal state preserved across frames.
#[expect(
    unnameable_types,
    reason = "Implementation detail, public because of trait visibility rules"
)]
#[allow(missing_debug_implementations)]
pub struct SignalMemoizeState<V, VState> {
    /// The currently rendered view.
    pub view: V,
    /// The inner view's state.
    pub view_state: VState,
    /// Signal version at last rebuild.
    pub last_version: u64,
    /// Set to true by RequestRebuild message.
    pub dirty: bool,
}

impl<V, ViewFn, Data, State, Action, Context> ViewMarker
    for SignalMemoize<V, ViewFn, Data, State, Action, Context>
{
}

impl<Data, V, ViewFn, State, Action, Context> View<State, Action, Context>
    for SignalMemoize<V, ViewFn, Data, State, Action, Context>
where
    State: 'static,
    Action: 'static,
    Context: ViewPathTracker + 'static,
    Data: 'static,
    V: View<State, Action, Context>,
    ViewFn: Fn(&Signal<Data>) -> V + 'static,
{
    type Element = V::Element;
    type ViewState = SignalMemoizeState<V, V::ViewState>;

    fn build(
        &self,
        ctx: &mut Context,
        app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let view = (self.view_fn)(&self.signal);
        let (element, view_state) = view.build(ctx, app_state);
        let memo_state = SignalMemoizeState {
            view,
            view_state,
            last_version: self.signal.version(),
            dirty: false,
        };
        (element, memo_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        // If the signal was replaced with a different Rc, force rebuild.
        if !Signal::ptr_eq(&self.signal, &prev.signal) {
            view_state.last_version = 0;
        }

        let current_version = self.signal.version();
        if core::mem::take(&mut view_state.dirty) || view_state.last_version != current_version {
            let view = (self.view_fn)(&self.signal);
            view.rebuild(
                &view_state.view,
                &mut view_state.view_state,
                ctx,
                element,
                app_state,
            );
            view_state.view = view;
            view_state.last_version = current_version;
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
    ) {
        view_state
            .view
            .teardown(&mut view_state.view_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let result = view_state.view.message(
            &mut view_state.view_state,
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
