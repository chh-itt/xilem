//! Memory footprint comparison.
//!
//! Run: cargo run --example bench_memory -p xilem_core_auralis --release

use std::cell::Cell;
use std::hint::black_box;
use std::mem::size_of;

use auralis_signal::Signal;
use xilem_core::{Environment, MessageCtx, MessageResult, Mut, View, ViewId, ViewPathTracker};
use xilem_core::memoize;
use xilem_core_auralis::{state_memoize, signal_state_memoize};

// Minimal View
struct Ctx;
impl ViewPathTracker for Ctx {
    fn environment(&mut self) -> &mut Environment { unimplemented!() }
    fn push_id(&mut self, _: ViewId) {}
    fn pop_id(&mut self) {}
    fn view_path(&mut self) -> &[ViewId] { &[] }
}

#[derive(Debug)]
struct V;
impl xilem_core::ViewMarker for V {}
impl View<(), (), Ctx> for V {
    type Element = xilem_core::NoElement; type ViewState = ();
    fn build(&self, _: &mut Ctx, _: &mut ()) -> (xilem_core::NoElement, ()) { unimplemented!() }
    fn rebuild(&self, _: &Self, _: &mut (), _: &mut Ctx, _: Mut<'_, xilem_core::NoElement>, _: &mut ()) {}
    fn teardown(&self, _: &mut (), _: &mut Ctx, _: Mut<'_, xilem_core::NoElement>) {}
    fn message(&self, _: &mut (), _: &mut MessageCtx, _: Mut<'_, xilem_core::NoElement>, _: &mut ()) -> MessageResult<()> { MessageResult::Nop }
}

fn v_i32(_: &i32) -> V { V }
fn v_str(_: &String) -> V { V }
fn v_sig(_: &Signal<i32>) -> V { V }

fn main() {
    println!("Memory footprint comparison (64-bit)\n");

    let m = memoize(black_box(42i32), v_i32);
    let ms = memoize(black_box("hello world!".to_string()), v_str);

    // Non-capturing closures are ZSTs (0 bytes)
    // Non-capturing closures = ZST (0 bytes)
    let sm_nc = state_memoize(
        |_: &()| black_box(42i32),
        |_: &()| black_box(0u64),
        v_i32,
    );

    // Realistic: closures that capture an index (like in a 1000-label list)
    let i: usize = 42;
    let sm_cap = state_memoize(
        move |_: &()| black_box(i as i32),
        move |_: &()| black_box(i as u64),
        v_i32,
    );

    let sms_nc = state_memoize(
        |_: &()| black_box("hello".to_string()),
        |_: &()| black_box(0u64),
        v_str,
    );

    let ssm_nc = signal_state_memoize(
        |_: &()| black_box(Signal::new(42i32)),
        v_sig,
    );

    println!("═══ View struct sizes ═══\n");
    println!("  memoize<i32>:                    {:>3} B  (i32:4B, fn ptr eliminated = ZST)",  size_of_val(&m));
    println!("  memoize<String>:                 {:>3} B  (String:24B, fn ptr eliminated = ZST)", size_of_val(&ms));
    println!("  state_memoize (non-capturing):   {:>3} B  (all closures ZST → 0B)", size_of_val(&sm_nc));
    println!("  state_memoize (captures index):  {:>3} B  (captures usize, 2× closure = 16B)", size_of_val(&sm_cap));
    println!("  signal_state_memoize (non-cap):  {:>3} B  (all closures ZST → 0B)", size_of_val(&ssm_nc));
    println!("  state_memoize<String> (non-cap): {:>3} B  (all closures ZST → 0B)", size_of_val(&sms_nc));

    println!("\n═══ Internal type sizes ═══\n");
    println!("  fn pointer:              {:>3} B", size_of::<fn(&())>());
    println!("  i32:                     {:>3} B", size_of::<i32>());
    println!("  String (stack):          {:>3} B  (ptr + len + cap)", size_of::<String>());
    println!("  Cell<u64>:               {:>3} B", size_of::<Cell<u64>>());
    println!("  Signal<i32>:             {:>3} B  (Rc<RefCell<SignalState>>)", size_of::<Signal<i32>>());
    println!("  usize (captured idx):    {:>3} B", size_of::<usize>());
    println!("  Non-capturing closure:    0 B  (ZST)");
    println!("  Capturing closure:     ~{:>3} B  (captured vars, no vtable)", size_of::<usize>());

    println!("\n═══ Per 1000 items ═══\n");
    println!("  {:<32} | {:>6} | {:>8} | {:>9}", "Type", "View", "+AppState", "= Total");
    println!("  {:-<65}", "");

    row("memoize i32", size_of_val(&m), 0);
    row("state_memoize i32 (capture idx)", size_of_val(&sm_cap), 16);
    row("signal_state_memoize i32", size_of_val(&ssm_nc), size_of::<Signal<i32>>());
    row("", 0, 0);
    row("memoize String", size_of_val(&ms), 0);
    row("state_memoize String", size_of_val(&sms_nc), 32); // String(24)+Cell(8)=32

    println!("\n  * memoize<String>: clones data into view each frame → heap alloc pressure");
    println!("  * state/signal views:  0-16B constant regardless of data size");
    println!("  * AppState overhead is one-time, shared across all frames");

    println!("\n═══ Takeaway ═══\n");
    println!("  memoize:              view grows with data → O(data_size) per frame");
    println!("  state_memoize:        constant 24B view → O(1) per frame");
    println!("  signal_state_memoize: constant 16B view + auto version tracking");
}

fn row(label: &str, view: usize, app: usize) {
    if label.is_empty() { println!("  {:-<60}", ""); return; }
    let total = (view + app) * 1000;
    println!("  {:<30} | {:>4} B | {:>5} B | {:>5} KB",
        label, view, app, total / 1024);
}
