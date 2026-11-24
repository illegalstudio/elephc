//! Thin executable entry point; compiler modules and dispatch live in the library.

/// A compile is a few hundred million short-lived strings and types, and the system allocator
/// was the largest single cost after copying them: 15% of the samples of a Symfony build.
#[cfg(not(feature = "mem-stats"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// `--features mem-stats` wraps the allocator in a live-byte counter for `ELEPHC_MEM_STATS=1`.
#[cfg(feature = "mem-stats")]
#[global_allocator]
static ALLOCATOR: elephc::mem_stats::counting::CountingAlloc =
    elephc::mem_stats::counting::CountingAlloc(mimalloc::MiMalloc);

fn main() {
    elephc::run_cli();
}
