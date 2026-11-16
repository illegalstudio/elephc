# Compiler-side Memoir extraction

- [x] Capture pre-extraction assembly fingerprints across supported targets and emission modes.
- [x] Extract heap allocation, refcounting and cycle-collector emitters into a compiler-only Rust library.
- [x] Wire the compiler emitter/ABI adapter and preserve existing runtime integration tests.
- [x] Document the crate boundary and validate unchanged assembly, focused runtime behavior and compiler builds.

## Scope

`elephc-memoir` is a compiler dependency with the ordinary Rust library crate type. It generates the existing assembly memory helpers and must never be a target-program bridge, static library, capability or `--with-` option. EIR ownership analysis and PHP-specific layout definitions remain in the compiler.

The initial extraction preserves allocation order, emitted instructions, symbols, PIC/dead-strip behavior, recoverable library boundaries, eval and mbstring retirement hooks, and all five supported targets. A narrow host trait delegates target syntax and ABI operations to the existing compiler infrastructure. Existing PHP layout contracts remain authoritative.

## Validation

Capture SHA-256 fingerprints before moving code, including internal-label bookkeeping. Re-run the same fingerprints after extraction. Run targeted allocator, ownership and cyclic-graph tests, cross-assemble memory helpers for every supported target, check crate metadata to exclude target-runtime artifacts, and run assembly comment hygiene and `git diff --check`. Use the machine-wide build queue for every compiling or linking command.

## Result

Completed on Linux x86_64 with Rust 1.99.1. The compiler builds without warnings.
The 30 pre-extraction assembly fingerprints match across the five supported
targets and six emission modes. Twenty-four distinct focused tests passed: three
Memoir boundary/fingerprint/header checks, fifteen cache/header/ABI/native-hash
checks, and six executable allocator/GC/reference/destructor/eval/metric regressions.
Clang successfully cross-assembled candidate collection, allocator split and tail
growth paths for macOS ARM64, iOS device and Simulator ARM64, Linux ARM64, and
Linux x86_64. The full suite was left to CI. Test bridge builds report the existing
Magician `fetch_update` deprecation under this host toolchain.

Cargo metadata exposes only an `rlib` target for Memoir, and no Memoir `.a` archive
is produced. Runtime-cache identity and heap-header hygiene cover the new source
owner. All registered memory emitter functions were accounted for; an unrelated,
unregistered legacy `gc_cycle_destructors.rs` remains unchanged and is tracked
separately in Code Journal task 5c6805c5.
