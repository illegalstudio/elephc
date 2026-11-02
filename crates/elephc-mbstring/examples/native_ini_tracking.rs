//! Purpose:
//! Measures native INI origin bookkeeping independently of PHP compilation and HTTP scheduling.
//!
//! Called from:
//! - `cargo run -p elephc-mbstring --release --example native_ini_tracking -- [iterations]`.
//!
//! Key details:
//! - Exercises the public C ABI with live, distinct allocation addresses and no INI resolution.
//! - Reports allocation calls and elapsed time; setup and warmup are outside each measured window.
//! - Timing measures registry callbacks only, not generated register-preservation hooks or HTTP.

use std::{alloc::{GlobalAlloc, Layout, System}, hint::black_box,
    sync::atomic::{AtomicU64, Ordering}, time::Instant};
use elephc_mbstring::abi::*;

struct CountingAllocator;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    /// Counts an allocation before forwarding its unchanged layout to the system allocator.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    /// Counts a zeroed allocation before forwarding it to the system allocator.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    /// Returns memory to the same system allocator that supplied it.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    /// Counts a resize as an allocation call, even if the allocator can grow in place.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Fails immediately if the benchmark violates the native identity ABI contract.
fn accepted(status: i32) { assert_eq!(black_box(status), 0); }

/// Measures repeated callbacks after warmup without counting output or timer initialization.
fn measure(name: &str, iterations: usize, mut cycle: impl FnMut()) {
    for _ in 0..100 { cycle(); }
    let started = Instant::now();
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..iterations { cycle(); }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    let elapsed = started.elapsed();
    println!("{name}: iterations={iterations} ns/cycle={:.2} allocations={allocations}",
        elapsed.as_nanos() as f64 / iterations as f64);
}

/// Exercises fresh churn, refresh, literal observation, aliasing, and request-sized map resets.
fn main() {
    let iterations = std::env::args().nth(1).map(|value| value.parse::<usize>().expect("positive iterations"))
        .unwrap_or(1_000_000);
    assert!(iterations >= 256);
    let storage = vec![[b'x'; 64]; 256];
    let owners: Vec<_> = storage.iter().map(|bytes| bytes.as_ptr() as u64).collect();
    let (source, destination) = (owners[0], owners[1]);

    measure("fresh_forget", iterations, || {
        accepted(elephc_mbstring_native_string_fresh_v1(source, 64));
        accepted(elephc_mbstring_native_string_forget_v1(source));
    });
    accepted(elephc_mbstring_native_string_fresh_v1(source, 64));
    measure("fresh_refresh", iterations, || accepted(elephc_mbstring_native_string_fresh_v1(source, 64)));
    accepted(elephc_mbstring_native_string_reset_v1());

    accepted(elephc_mbstring_native_string_literal_v1(source, 64));
    measure("literal_observation", iterations, || accepted(elephc_mbstring_native_string_literal_v1(source, 64)));
    accepted(elephc_mbstring_native_string_reset_v1());

    accepted(elephc_mbstring_native_string_fresh_v1(source, 64));
    measure("persist_forget", iterations, || {
        accepted(elephc_mbstring_native_string_persist_v1(destination, source, 64));
        accepted(elephc_mbstring_native_string_forget_v1(destination));
    });
    accepted(elephc_mbstring_native_string_reset_v1());

    accepted(elephc_mbstring_native_string_fresh_v1(source, 64));
    measure("fresh_persist_forget", iterations, || {
        accepted(elephc_mbstring_native_string_fresh_v1(source, 64));
        accepted(elephc_mbstring_native_string_persist_v1(destination, source, 64));
        accepted(elephc_mbstring_native_string_forget_v1(destination));
    });
    accepted(elephc_mbstring_native_string_reset_v1());

    measure("request_256_fresh_reset", iterations / owners.len(), || {
        for &owner in &owners { accepted(elephc_mbstring_native_string_fresh_v1(owner, 64)); }
        accepted(elephc_mbstring_native_string_reset_v1());
    });
    accepted(elephc_mbstring_native_string_reset_v1());
    black_box(storage);
}
