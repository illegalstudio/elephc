# EIR tail-call optimization

- [x] Implement safe direct tail-call discovery and recursive component analysis.
- [x] Reuse EIR frames through entry branches and simultaneous parameter rebinding.
- [x] Cover self recursion, mutual recursion, evaluation order and rejection guards.
- [x] Validate supported-target emission and focused runtime tests.
- [x] Update optimizer documentation, roadmap and a runnable example.

## Design

Run a module-level transform after the ordinary EIR optimization fixed point and
before ownership finalization. Recognize resolved user-function calls whose
result is returned directly, with no observable work after the call. Build
recursive components from eligible tail edges. Each original function remains
an ABI entry point; mutual components transplant their bodies into each entry
point so recursive transitions become ordinary intra-function `Br` edges.

Each body has a rebinding block with typed block parameters. Argument evaluation
stays before the branch, block argument moves are simultaneous, and the rebinding
block stores the new parameter values before entering the body. A separate
initial entry preserves the original ABI parameter initialization. Fresh scalar
locals are zeroed on every activation; all load effects are restored before
optimizing the new loops.

Start with scalar (int, bool, float, void) boundaries and scalar local storage.
Retain ordinary calls for reference/variadic signatures, owning frame state,
exception handlers, generators, closures, methods, argument introspection,
dynamic eval, unsupported coercions and non-tail calls. Numeric argument owners
qualify only after proving slot and unwind-record balance on every normal CFG
path; their existing ABI conversion precedes retirement. Disable the transform when PHP
backtraces or eval can observe the physical activation chain. Bound mutual-body
cloning to avoid quadratic growth on unusually large recursive components.

No target-specific instructions are introduced. Existing EIR branch lowering,
parallel moves, frame layout and register allocation remain authoritative for
all five supported targets. Tests must verify deep recursion exceeds ordinary
stack limits and that optimized/unoptimized bounded programs agree.


## Validation

- `cargo build -j 2`: warning-free.
- `cargo test -p elephc --lib tail_call -j 2`: structural, negative and all-target emitter checks.
- `cargo test -p elephc --lib codegen::block_layout::tests -j 2`: declared-entry and layout checks.
- `cargo test -p elephc --test codegen_tests optimizer::tail_call -j 2`: 12 passed.
- `cargo test -p elephc --test codegen_tests stack_guard -j 2`: 9 passed.
- `cargo test -p elephc --test codegen_tests optimizer::inline -j 2`: 16 passed, 1 pre-existing ignored test.
- The float/boolean/integer parameter swap fixture also passed with `ELEPHC_REGALLOC=stack`.
- A 300000-step ternary return fixture passed through the CLI with both linear and stack allocation.
- PHP cross-checks passed for bounded swaps, named/default arguments, local reset, cleanup/reference and backtrace fixtures.
- Small-heap, heap-debug and allocation/free balance checks passed.
- `git diff --check`, assembly-comment alignment and example PHP syntax checks passed.

Executable validation ran on Linux x86_64. Emitter tests cover macOS ARM64,
iOS device ARM64, iOS Simulator ARM64, Linux ARM64 and Linux x86_64.
