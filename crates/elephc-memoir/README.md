# elephc-memoir

Compiler-side assembly generation for Elephc memory management. This crate is
an ordinary Rust `rlib` dependency of the compiler. It is never linked into
compiled PHP programs and produces no runtime static archive or `--with-memoir`
capability. Generated programs continue to use the same emitted `__rt_*` helpers.

## Boundary

Memoir owns heap allocation and consolidation, allocator diagnostics and metrics,
reference counting, cycle collection and candidate proofs, graph traversal, and
resumable cleanup scopes. The `Emitter` trait delegates assembly output, symbol
spelling, library-boundary modes and target ABI operations to the compiler.

The compiler retains EIR lifetime analysis, ownership optimization and placement,
PHP class/coroutine/SPL layout definitions, runtime data emission, runtime helper
ordering, runtime object caching, and final linking. Type-specific operations such
as string persistence and array/hash copy-on-write and deep release stay with
their PHP runtime modules in this phase. `GraphLayout` and
`GcStatusSelectors` supply compiler-owned facts without importing compiler types.
Canonical heap-kind and boundary constants live in Memoir and remain available
through the compiler's compatibility re-exports. The former memory modules under
`src/codegen_support/runtime/arrays` retain thin re-exports and integration tests;
they contain no duplicate implementation.

This first extraction preserves emitted instructions, labels and ownership
behavior. Existing compiler integration tests use the actual target emitter;
assembly fingerprints cover all five supported targets and executable, PIC,
recoverable-library, hosted-extension and dead-strip configurations.

`build.rs` includes this crate's Rust sources in the runtime-cache identity.
Changes to these emitters therefore invalidate cached runtime objects even when
no compiler-local emitter source changes.
