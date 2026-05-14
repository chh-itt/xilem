use core::fmt::Debug;
use core::marker::PhantomData;

use auralis_signal::Signal;
use xilem_core::{MessageCtx, MessageResult, Mut, View, ViewMarker, ViewPathTracker};

/// A memoize view that gets a `Signal<Data>` from `&State` via a function
/// pointer.  The `Signal` itself lives in `State` (not in the view), so
/// the view remains `Send + Sync`.
///
/// This eliminates manual version management: `Signal::set()` auto-increments
/// the version.  The `Signal` is `!Send + !Sync`, but that's fine because
/// it lives in `State` which has no `Send`/`Sync` requirement.
///
/// # Example
///
/// ```ignore
/// use auralis_signal::Signal;
/// use xilem_core_auralis::signal_state_memoize;
///
/// struct AppState {
///     count: Signal<i32>,
/// }
///
/// fn counter_view(_state: &AppState) -> impl WidgetView<AppState> {
///     signal_state_memoize(
///         |s: &AppState| s.count.clone(),
///         |sig: &Signal<i32>| label(format!("count: {}", sig.read())),
///     )
/// }
/// ```
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SignalStateMemoize<V, GetSignal, ViewFn, Data, State, Action, Context> {
    get_signal: GetSignal,
    view_fn: ViewFn,
    phantom: PhantomData<fn() -> (V, Data, State, Action, Context)>,
}

impl<V, GetSignal, ViewFn, Data, State, Action, Context> Debug
    for SignalStateMemoize<V, GetSignal, ViewFn, Data, State, Action, Context>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SignalStateMemoize").finish_non_exhaustive()
    }
}

/// Create a [`SignalStateMemoize`] view.
///
/// - `get_signal` — extracts a `Signal<Data>` from `&State`
/// - `view_fn` — builds a view from `&Signal<Data>`
///
/// The signal's version is used for O(1) change detection.  No manual
/// version counter needed — `Signal::set()` handles it automatically.
pub fn signal_state_memoize<GetSignal, ViewFn, Data, V, State, Action, Context>(
    get_signal: GetSignal,
    view_fn: ViewFn,
) -> SignalStateMemoize<V, GetSignal, ViewFn, Data, State, Action, Context>
where
    State: 'static,
    Data: 'static,
    GetSignal: Fn(&State) -> Signal<Data> + Send + Sync + 'static,
    ViewFn: Fn(&Signal<Data>) -> V + Send + Sync + 'static,
    V: View<State, Action, Context>,
    Context: ViewPathTracker,
{
    SignalStateMemoize {
        get_signal,
        view_fn,
        phantom: PhantomData,
    }
}

#[expect(
    unnameable_types,
    reason = "Implementation detail, public because of trait visibility rules"
)]
#[allow(missing_debug_implementations)]
pub struct SignalStateMemoizeState<V, VState> {
    pub view: V,
    pub view_state: VState,
    pub last_version: u64,
    pub dirty: bool,
}

impl<V, GetSignal, ViewFn, Data, State, Action, Context> ViewMarker
    for SignalStateMemoize<V, GetSignal, ViewFn, Data, State, Action, Context>
{
}

impl<GetSignal, ViewFn, Data, V, State, Action, Context> View<State, Action, Context>
    for SignalStateMemoize<V, GetSignal, ViewFn, Data, State, Action, Context>
where
    State: 'static,
    Action: 'static,
    Context: ViewPathTracker + 'static,
    Data: 'static,
    GetSignal: Fn(&State) -> Signal<Data> + Send + Sync + 'static,
    ViewFn: Fn(&Signal<Data>) -> V + Send + Sync + 'static,
    V: View<State, Action, Context>,
{
    type Element = V::Element;
    type ViewState = SignalStateMemoizeState<V, V::ViewState>;

    fn build(
        &self,
        ctx: &mut Context,
        app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let signal = (self.get_signal)(app_state);
        let version = signal.version();
        let view = (self.view_fn)(&signal);
        let (element, view_state) = view.build(ctx, app_state);
        let memo_state = SignalStateMemoizeState {
            view,
            view_state,
            last_version: version,
            dirty: false,
        };
        (element, memo_state)
    }

    fn rebuild(
        &self,
        _prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let signal = (self.get_signal)(app_state);
        let current_version = signal.version();
        if core::mem::take(&mut view_state.dirty) || view_state.last_version != current_version {
            let view = (self.view_fn)(&signal);
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
