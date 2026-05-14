use core::fmt::Debug;
use core::marker::PhantomData;

use xilem_core::{MessageCtx, MessageResult, Mut, View, ViewMarker, ViewPathTracker};

/// A memoize view that gets data and version from AppState via function pointers.
///
/// Unlike [`SignalMemoize`](crate::SignalMemoize), this does NOT store a
/// `Signal` internally, so it is `Send + Sync` when the function pointers
/// are.  The tradeoff: the caller must manually increment a version counter
/// alongside each data mutation.
///
/// Use this when you need `WidgetView` compatibility (e.g. Xilem with winit).
///
/// # Example
///
/// ```ignore
/// use std::cell::Cell;
/// use xilem_core_auralis::state_memoize;
///
/// struct AppState {
///     count: i32,
///     version: Cell<u64>,
/// }
///
/// fn counter_view(state: &AppState) -> impl WidgetView<AppState> {
///     state_memoize(
///         |s: &AppState| s.count,
///         |s: &AppState| s.version.get(),
///         |count: &i32| label(format!("count: {count}")),
///     )
/// }
/// ```
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct StateMemoize<V, GetData, GetVersion, ViewFn, Data, State, Action, Context> {
    get_data: GetData,
    get_version: GetVersion,
    view_fn: ViewFn,
    phantom: PhantomData<fn() -> (V, Data, State, Action, Context)>,
}

impl<V, GetData, GetVersion, ViewFn, Data, State, Action, Context> Debug
    for StateMemoize<V, GetData, GetVersion, ViewFn, Data, State, Action, Context>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StateMemoize").finish_non_exhaustive()
    }
}

/// Create a [`StateMemoize`] view.
///
/// - `get_data` — extracts data from `&State`
/// - `get_version` — extracts version from `&State`
/// - `view_fn` — builds a view from the extracted data
///
/// When the version changes between rebuilds, the view is re-created.
pub fn state_memoize<GetData, GetVersion, ViewFn, Data, V, State, Action, Context>(
    get_data: GetData,
    get_version: GetVersion,
    view_fn: ViewFn,
) -> StateMemoize<V, GetData, GetVersion, ViewFn, Data, State, Action, Context>
where
    State: 'static,
    Data: 'static,
    GetData: Fn(&State) -> Data + Send + Sync + 'static,
    GetVersion: Fn(&State) -> u64 + Send + Sync + 'static,
    ViewFn: Fn(&Data) -> V + Send + Sync + 'static,
    V: View<State, Action, Context>,
    Context: ViewPathTracker,
{
    StateMemoize {
        get_data,
        get_version,
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
pub struct StateMemoizeState<V, VState> {
    /// The currently rendered view.
    pub view: V,
    /// The inner view's state.
    pub view_state: VState,
    /// Version at last rebuild.
    pub last_version: u64,
    /// Set to true by RequestRebuild message.
    pub dirty: bool,
}

impl<V, GetData, GetVersion, ViewFn, Data, State, Action, Context> ViewMarker
    for StateMemoize<V, GetData, GetVersion, ViewFn, Data, State, Action, Context>
{
}

impl<GetData, GetVersion, ViewFn, Data, V, State, Action, Context> View<State, Action, Context>
    for StateMemoize<V, GetData, GetVersion, ViewFn, Data, State, Action, Context>
where
    State: 'static,
    Action: 'static,
    Context: ViewPathTracker + 'static,
    Data: 'static,
    GetData: Fn(&State) -> Data + Send + Sync + 'static,
    GetVersion: Fn(&State) -> u64 + Send + Sync + 'static,
    ViewFn: Fn(&Data) -> V + Send + Sync + 'static,
    V: View<State, Action, Context>,
{
    type Element = V::Element;
    type ViewState = StateMemoizeState<V, V::ViewState>;

    fn build(
        &self,
        ctx: &mut Context,
        app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let data = (self.get_data)(app_state);
        let version = (self.get_version)(app_state);
        let view = (self.view_fn)(&data);
        let (element, view_state) = view.build(ctx, app_state);
        let memo_state = StateMemoizeState {
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
        let current_version = (self.get_version)(app_state);
        if core::mem::take(&mut view_state.dirty) || view_state.last_version != current_version {
            let data = (self.get_data)(app_state);
            let view = (self.view_fn)(&data);
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
