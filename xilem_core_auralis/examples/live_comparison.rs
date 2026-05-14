//! Live side-by-side: xilem_core::memoize vs xilem_core_auralis::signal_state_memoize.
//!
//! Run with: cargo run --example live_comparison -p xilem_core_auralis
//!
//! Key difference:
//!   Left:  memoize(data, |d| view) — needs Data: PartialEq
//!   Right: signal_state_memoize(|s| s.sig, |sig| view) — auto version tracking
//!
//! Signal lives in AppState (no Send+Sync issue), accessed via fn pointer.
//! Zero manual version management: Signal::set() auto-increments the version.

use auralis_signal::Signal;
use winit::dpi::LogicalSize;
use xilem::view::{flex_col, flex_row, label, text_button, FlexSpacer};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};
use xilem_core::memoize;
use xilem_core_auralis::signal_state_memoize;

struct AppState {
    // Xilem side: plain i32
    xilem_count: i32,
    // Auralis side: Signal<i32> — auto version tracking, no manual Cell
    auralis_count: Signal<i32>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            xilem_count: 0,
            auralis_count: Signal::new(0),
        }
    }
}

fn xilem_counter(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("=== xilem_core::memoize ===").text_size(16.0),
        label("Data: PartialEq needed. Data stored in Memoize struct.").text_size(11.0),
        memoize(state.xilem_count, |count| {
            flex_row((
                label(format!("Count: {count}")),
                FlexSpacer::Flex(1.0),
                text_button("-", |state: &mut AppState| state.xilem_count -= 1),
                text_button("+", |state: &mut AppState| state.xilem_count += 1),
            ))
        }),
    ))
}

fn auralis_counter(_state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("=== auralis::signal_state_memoize ===").text_size(16.0),
        label("Signal<Data> in AppState. Auto version tracking.").text_size(11.0),
        signal_state_memoize(
            |s: &AppState| s.auralis_count.clone(),
            |count: &Signal<i32>| {
                flex_row((
                    label(format!("Count: {}", count.read())),
                    FlexSpacer::Flex(1.0),
                    // Callback accesses Signal via &mut AppState (doesn't capture Signal directly)
                    text_button("-", |state: &mut AppState| {
                        state.auralis_count.set(state.auralis_count.read() - 1);
                    }),
                    text_button("+", |state: &mut AppState| {
                        state.auralis_count.set(state.auralis_count.read() + 1);
                    }),
                ))
            },
        ),
    ))
}

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_row((
        flex_col((xilem_counter(state), FlexSpacer::Flex(1.0))),
        flex_col((
            auralis_counter(state),
            FlexSpacer::Flex(1.0),
            label(
                "\nSignal<i32> lives in AppState (not in View).\n\
                 fn pointer to extract it → View stays Send+Sync.\n\
                 Signal::set() auto-increments version → no manual Cell<u64>.\n\
                 O(1) version comparison vs O(n) PartialEq.\n\
                 \n\
                 Same API as real Signal usage in Auralis.",
            )
            .text_size(11.0),
        )),
    ))
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(
        AppState::new(),
        app_logic,
        WindowOptions::new("Xilem memoize vs Auralis signal_state_memoize")
            .with_min_inner_size(LogicalSize::new(900., 400.)),
    );
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
