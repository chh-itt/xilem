//! Arc::ptr_eq vs Signal::version() vs memoize PartialEq.
//!
//! Run: cargo run --example bench_arc -p xilem_core_auralis --release
//!
//! Xilem already has O(1) memoization via Arc<impl View> + ptr_eq.
//! This benchmark compares all three approaches at the "should I rebuild?" check.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use auralis_signal::Signal;

const N: u32 = 2_000_000;

fn main() {
    println!("Change-detection: Arc::ptr_eq vs Signal::version vs memoize PartialEq\n");
    println!("{N} iterations per test, release build\n");

    // ===== Raw comparison: Arc::ptr_eq =====
    let arc1 = Arc::new(42i32);
    let arc2 = Arc::clone(&arc1); // same allocation
    let arc3 = Arc::new(42i32);   // different allocation, same value

    let t = measure(|| for _ in 0..N {
        black_box(Arc::ptr_eq(black_box(&arc1), black_box(&arc2)));
    });
    let per = t.as_nanos() as f64 / N as f64;
    println!("  Arc::ptr_eq (same ptr):           {:>8.2?}  ({:.1}ns/check)", t, per);

    let t = measure(|| for _ in 0..N {
        black_box(Arc::ptr_eq(black_box(&arc1), black_box(&arc3)));
    });
    let per = t.as_nanos() as f64 / N as f64;
    println!("  Arc::ptr_eq (different ptr):       {:>8.2?}  ({:.1}ns/check)", t, per);

    // ===== Raw comparison: Signal::version =====
    let sig = Signal::new(42i32);
    let v = sig.version();

    let t = measure(|| for _ in 0..N {
        black_box(black_box(v) == black_box(sig.version()));
    });
    let per = t.as_nanos() as f64 / N as f64;
    println!("  Signal::version (same):            {:>8.2?}  ({:.1}ns/check)", t, per);

    sig.set(99);
    let t = measure(|| for _ in 0..N {
        black_box(black_box(v) != black_box(sig.version()));
    });
    let per = t.as_nanos() as f64 / N as f64;
    println!("  Signal::version (different):       {:>8.2?}  ({:.1}ns/check)", t, per);

    // ===== String PartialEq =====
    let s1 = "The quick brown fox jumps over the lazy dog. ".repeat(250);
    let s2 = s1.clone();

    let t = measure(|| for _ in 0..N {
        black_box(black_box(&s1) == black_box(&s2));
    });
    let per = t.as_nanos() as f64 / N as f64;
    println!("  String 12KB PartialEq (equal):     {:>8.2?}  ({:.1}ns/check)", t, per);

    // ===== Summary =====
    println!("\n═══ Summary ═══\n");
    println!("  Arc::ptr_eq is O(1) — same as Signal::version.");
    println!("  The difference is NOT in the comparison cost.");
    println!("  It's in who decides *when* to create a new Arc/Signal:");
    println!();
    println!("  Arc<impl View>:  User must manually compare data,");
    println!("                    create new Arc if changed, clone if not.");
    println!("                    → Manual lifecycle management.");
    println!();
    println!("  Signal + Memo:   Signal::set() → version auto-increments.");
    println!("                    Memo reads signals, auto-tracks deps.");
    println!("                    → Automatic lifecycle.");
    println!();
    println!("  memoize:         Framework compares Data: PartialEq each rebuild.");
    println!("                    Requires PartialEq. O(n) for large data.");
    println!("                    → Automatic check, O(n) cost.");
}

fn measure(f: impl FnOnce()) -> std::time::Duration {
    let start = Instant::now();
    f();
    start.elapsed()
}
