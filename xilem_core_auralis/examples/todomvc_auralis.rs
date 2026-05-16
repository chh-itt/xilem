//! Signal-driven components inside Xilem/Masonry.
//!
//! Run with: cargo run --example todomvc_auralis -p xilem_core_auralis
//!
//! Left panel:  standard xilem_core::memoize
//! Right panel: Auralis signal_state_memoize
//!
//! Both panels share the same two signals (count + toggle) via
//! Signal::clone() — no lens, no message wiring.
//!
//! Press "Print DevTools Snapshot" to dump the reactive graph to stdout.

use auralis_devtools;
use auralis_signal::Signal;
use winit::dpi::LogicalSize;
use xilem::view::{flex_col, flex_row, label, text_button, FlexSpacer};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};
use xilem_core::memoize;
use xilem_core_auralis::signal_state_memoize;

struct AppState {
    xilem_count: i32,
    auralis_count: Signal<i32>,
    auralis_toggle: Signal<bool>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            xilem_count: 0,
            auralis_count: Signal::new(0),
            auralis_toggle: Signal::new(false),
        }
    }
}

// -- Left panel: xilem_core::memoize -------------------------------------

fn xilem_panel(state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("xilem_core::memoize").text_size(16.0),
        memoize(state.xilem_count, |count| {
            label(format!("Counter: {count}"))
        }),
        flex_row((
            text_button("-", |s: &mut AppState| s.xilem_count -= 1),
            text_button("+", |s: &mut AppState| s.xilem_count += 1),
        )),
    ))
}

// -- Right panel: Auralis signal_state_memoize ----------------------------

fn auralis_panel(_state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("auralis::signal_state_memoize").text_size(16.0),
        signal_state_memoize(
            |s: &AppState| s.auralis_count.clone(),
            |count: &Signal<i32>| label(format!("Counter: {}", count.read())),
        ),
        flex_row((
            text_button("-", |s: &mut AppState| {
                s.auralis_count.set(s.auralis_count.read() - 1);
            }),
            text_button("+", |s: &mut AppState| {
                s.auralis_count.set(s.auralis_count.read() + 1);
            }),
        )),
        signal_state_memoize(
            |s: &AppState| s.auralis_toggle.clone(),
            |toggled: &Signal<bool>| label(format!("Toggle: {}", toggled.read())),
        ),
        text_button("Toggle", |s: &mut AppState| {
            let cur = s.auralis_toggle.read();
            s.auralis_toggle.set(!cur);
        }),
    ))
}

// -- Cross-component signal sharing ---------------------------------------

fn shared_view(_state: &AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("Cross-Component Signal Sharing").text_size(16.0),
        signal_state_memoize(
            |s: &AppState| s.auralis_count.clone(),
            |count: &Signal<i32>| label(format!("  same count signal → {}", count.read())),
        ),
        signal_state_memoize(
            |s: &AppState| s.auralis_toggle.clone(),
            |toggled: &Signal<bool>| label(format!("  same toggle signal → {}", toggled.read())),
        ),
    ))
}

// -- DevTools ------------------------------------------------------------

fn devtools_section() -> impl WidgetView<AppState> {
    text_button(
        "Print DevTools Snapshot",
        |_: &mut AppState| {
            let snap = auralis_devtools::snapshot();
            println!(
                "\n=== DevTools Snapshot ===\nSignals: {}  Memos: {}  Scopes: {}\n",
                snap.signals.len(),
                snap.memos.len(),
                snap.scope_tree.len(),
            );
            for s in &snap.signals {
                println!(
                    "  Signal addr={} ver={} subs={} type={}",
                    s.addr, s.version, s.subscriber_count, s.type_name,
                );
            }
            for m in &snap.memos {
                println!(
                    "  Memo   addr={} ver={} deps={} dirty={} computed={}x",
                    m.addr, m.version, m.dependency_count, m.is_dirty, m.compute_count,
                );
            }
            if !snap.derivation_tree.is_empty() {
                println!("\nDerivation tree:");
                if let Ok(v) = serde_json::to_value(&snap.derivation_tree) {
                    print_tree(&v, 0);
                }
            }
            println!();
        },
    )
}

fn print_tree(value: &serde_json::Value, depth: usize) {
    let indent = "  ".repeat(depth);
    match value {
        serde_json::Value::Array(arr) => {
            for v in arr {
                print_tree(v, depth);
            }
        }
        serde_json::Value::Object(map) => {
            let ty = map.get("node_type").and_then(|v| v.as_str()).unwrap_or("?");
            let addr = map.get("addr").and_then(|v| v.as_str()).unwrap_or("?");
            let ver = map.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
            let kids = map
                .get("depended_by")
                .and_then(|v| v.as_array())
                .map_or(0, |a| a.len());
            println!("{indent}{ty} addr={addr} ver={ver} depended_by={kids}");
            if let Some(children) = map.get("depended_by") {
                print_tree(children, depth + 1);
            }
        }
        _ => {}
    }
}

// -- Root -----------------------------------------------------------------

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    flex_col((
        label("Auralis Signal Components").text_size(20.0),
        flex_row((
            xilem_panel(state),
            FlexSpacer::Flex(0.3),
            auralis_panel(state),
        )),
        FlexSpacer::Flex(0.3),
        shared_view(state),
        FlexSpacer::Flex(0.3),
        devtools_section(),
    ))
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(
        AppState::new(),
        app_logic,
        WindowOptions::new("Auralis Signal Components")
            .with_min_inner_size(LogicalSize::new(800., 500.)),
    );
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
