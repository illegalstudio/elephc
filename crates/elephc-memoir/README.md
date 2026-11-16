# elephc-memoir

Compiler-side assembly generation for Elephc memory management. This crate is
an ordinary Rust `rlib` dependency of the compiler. It is never linked into
compiled PHP programs and produces no runtime static archive or `--with-memoir`
capability. Generated programs continue to use the same emitted `__rt_*` helpers.

## Boundary

Memoir owns heap allocation and consolidation, allocator diagnostics and metrics,
reference counting, cycle collection and candidate proofs, graph traversal,
container allocation and growth, copy-on-write and cloning, deep release, owned
strings and scratch allocation, growable output buffers, consuming string append,
callable-descriptor retirement, hash-entry ownership guards, generation-safe
buffers, and reference-cell lifetime and pin primitives. It also emits fixed heap, GC, buffer, reference-root
and string-scratch declarations, keeping their existing order and byte layout.

The `Emitter` trait delegates assembly output, symbol spelling, library-boundary
modes, compiler-owned layout facts and target ABI operations to the compiler.
`DataTarget` delegates object-format syntax. `ValueFeatures`, `GraphLayout` and
`GcStatusSelectors` carry the small set of compiler facts needed by the emitters
without importing PHP types or EIR.

The compiler retains EIR lifetime analysis, ownership optimization and placement,
PHP reference/type validation, class/coroutine/SPL layout definitions, semantic
runtime data, helper ordering, runtime object caching, and final linking.
Memory constants live in Memoir with compatibility re-exports where needed.
The former compiler memory modules retain thin adapters and integration tests;
they contain no duplicate implementation.

The extractions preserve emitted instructions, labels and ownership behavior.
Frozen assembly fingerprints cover all five supported targets and executable,
PIC, recoverable-library, hosted-extension and dead-strip configurations. The
value-storage fingerprints also cover optional resource hooks and the complete
fixed runtime-data block. Storage fingerprints additionally cover growable
buffers, append metadata variants, callable capture release and hash guards.

`build.rs` includes this crate's Rust sources in the runtime-cache identity.
Changes to these emitters therefore invalidate cached runtime objects even when
no compiler-local emitter source changes.
