//! Thin executable entry point; compiler modules and dispatch live in the library.

/// A compile is a few hundred million short-lived strings and types, and the system allocator
/// was the largest single cost after copying them: 15% of the samples of a Symfony build.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    elephc::run_cli();
}
