use core::fmt::Debug;
use core::marker::PhantomData;

use auralis_signal::Memo;
use xilem_core::{MessageCtx, MessageResult, Mut, View, ViewMarker, ViewPathTracker};

/// A view that uses a [`Memo<()>`] as a dependency tracker.
///
/// The `tracker` memo should read all signals whose changes should
/// trigger a rebuild.  When any tracked signal changes, the view
/// re-runs `view_fn` and calls `rebuild` on the new view.
///
/// This gives **auto-tracking** — the caller doesn't need to manually
/// list dependencies.  The Memo's internal observer records every
/// signal read during its first evaluation.
///
/// # Example
///
/// ```ignore
/// use auralis_signal::{Signal, Memo};
/// use xilem_core_auralis::effect_view;
///
/// let count = Signal::new(0);
/// let name = Signal::new("Alice");
///
/// // Memo auto-tracks both `count` and `name`
/// let tracker = Memo::new(move || {
///     let _ = count.read();
///     let _ = name.read();
/// });
///
/// let view = effect_view(tracker, || {
///     label(format!("{}: {}", name.read_untracked(), count.read_untracked()))
/// });
/// ```
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct EffectView<V, ViewFn, State, Action, Context> {
    tracker: Memo<()>,
    view_fn: ViewFn,
    phantom: PhantomData<fn() -> (V, State, Action, Context)>,
}

impl<V, ViewFn, State, Action, Context> Debug
    for EffectView<V, ViewFn, State, Action, Context>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EffectView")
            .field("is_dirty", &self.tracker.is_dirty())
            .finish_non_exhaustive()
    }
}

#[expect(
    unnameable_types,
    reason = "Implementation detail, public because of trait visibility rules"
)]
#[allow(missing_debug_implementations)]
pub struct EffectViewState<V, VState> {
    view: V,
    view_state: VState,
    dirty: bool,
}

/// Create an [`EffectView`].
///
/// `tracker` is a `Memo<()>` whose compute closure reads all the
/// signals this view depends on.  When any dependency changes, the
/// tracker becomes dirty and the view is rebuilt.
pub fn effect_view<V, ViewFn, State, Action, Context>(
    tracker: Memo<()>,
    view_fn: ViewFn,
) -> EffectView<V, ViewFn, State, Action, Context>
where
    State: 'static,
    ViewFn: Fn() -> V + 'static,
    V: View<State, Action, Context>,
    Context: ViewPathTracker,
{
    EffectView {
        tracker,
        view_fn,
        phantom: PhantomData,
    }
}

impl<V, ViewFn, State, Action, Context> ViewMarker
    for EffectView<V, ViewFn, State, Action, Context>
{
}

impl<V, ViewFn, State, Action, Context> View<State, Action, Context>
    for EffectView<V, ViewFn, State, Action, Context>
where
    State: 'static,
    Action: 'static,
    Context: ViewPathTracker + 'static,
    V: View<State, Action, Context>,
    ViewFn: Fn() -> V + 'static,
{
    type Element = V::Element;
    type ViewState = EffectViewState<V, V::ViewState>;

    fn build(
        &self,
        ctx: &mut Context,
        app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        // Force initial evaluation of the tracker to register dependencies
        let _ = self.tracker.read();
        let view = (self.view_fn)();
        let (element, view_state) = view.build(ctx, app_state);
        let state = EffectViewState {
            view,
            view_state,
            dirty: false,
        };
        (element, state)
    }

    fn rebuild(
        &self,
        _prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut Context,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        // Check if any tracked dependency changed
        if core::mem::take(&mut view_state.dirty) || self.tracker.is_dirty() {
            // Re-read to acknowledge the change and re-register dependencies
            let _ = self.tracker.read();
            let view = (self.view_fn)();
            view.rebuild(
                &view_state.view,
                &mut view_state.view_state,
                ctx,
                element,
                app_state,
            );
            view_state.view = view;
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
