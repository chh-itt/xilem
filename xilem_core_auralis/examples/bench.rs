//! Change-detection cost: PartialEq vs u64 version comparison.
//!
//! Run: cargo run --example bench -p xilem_core_auralis --release
//!
//! This measures the cost of the "should I rebuild?" check inside
//! View::rebuild. The check is the ONLY difference between memoize
//! and signal/state memoize — the rest of rebuild (context setup,
//! inner view rebuild, etc.) is identical for both.
//!
//! For memoize:   `if prev.data != self.data { ... }`  (PartialEq)
//! For our views:  `if last_version != current_version { ... }` (u64 cmp)

use std::hint::black_box;
use std::time::Instant;

const N: u32 = 2_000_000;

fn main() {
    println!("Change-detection: PartialEq vs u64 version comparison\n");
    println!("{N} iterations per test, release build\n");

    header("SAME VALUE (typical frame: data unchanged, skip rebuild)");

    cmp("i32", &42, &42);
    cmp("String 16B", &"hello world!    ".to_string(), &"hello world!    ".to_string());
    cmp("String 64B", &"The quick brown fox jumps over the lazy dog.  ".to_string(),
                        &"The quick brown fox jumps over the lazy dog.  ".to_string());
    cmp("String 1KB", &"Lorem ipsum. ".repeat(64), &"Lorem ipsum. ".repeat(64));
    cmp("String 12KB", &"The quick brown fox. ".repeat(500),
                       &"The quick brown fox. ".repeat(500));

    header("DIFFERENT VALUE (data changed, rebuild triggered)");

    cmp("i32 (first differs)", &0, &1);
    cmp("String 16B (first char)", &"hello world!    ".to_string(), &"HELLO WORLD!    ".to_string());
    cmp("String 1KB (first char)", &"Lorem ipsum. ".repeat(64), &"LOREM IPSUM. ".repeat(64));

    header("LARGE COLLECTIONS");

    let v100a: Vec<String> = (0..100).map(|i| format!("item-{i:04}")).collect();
    let v100b: Vec<String> = (0..100).map(|i| format!("item-{i:04}")).collect();
    cmp("Vec<String> 100 (~3.5KB)", &v100a, &v100b);

    let v1ka: Vec<String> = (0..1000).map(|i| format!("item-{i:06}")).collect();
    let v1kb: Vec<String> = (0..1000).map(|i| format!("item-{i:06}")).collect();
    cmp("Vec<String> 1000 (~35KB)", &v1ka, &v1kb);
}

fn header(s: &str) {
    println!("\n═══ {s} ═══\n");
}

fn cmp<T: PartialEq>(label: &str, a: &T, b: &T) {
    // PartialEq: compare two values
    let t = measure(|| {
        for _ in 0..N {
            black_box(black_box(a) == black_box(b));
        }
    });

    // Version: compare two u64
    let t_ver = measure(|| {
        for _ in 0..N {
            black_box(black_box(42u64) == black_box(42u64));
        }
    });

    let ratio = t.as_nanos() as f64 / t_ver.as_nanos().max(1) as f64;
    let note = if ratio < 1.5 { "same ballpark" }
        else if ratio < 10.0 { "noticeable" }
        else if ratio < 100.0 { "significant" }
        else { "massive gap" };
    println!(
        "  {:<32} | PartialEq: {:>8.2?} | Version: {:>8.2?} | {:>5.0}x ({})",
        label, t, t_ver, ratio, note
    );
}

fn measure(f: impl FnOnce()) -> std::time::Duration {
    let start = Instant::now();
    f();
    start.elapsed()
}
