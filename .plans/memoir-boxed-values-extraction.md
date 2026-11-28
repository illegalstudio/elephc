# Memoir boxed values and protected reference release

- [x] Capture pre-extraction fingerprints for boxed-value constructors, persistent references and protected release.
- [x] Extract Mixed constructors, persistent-reference helpers, local reference retirement and exception-preserving owner cleanup.
- [x] Keep resource identity policy and exception machinery in the compiler through explicit host boundaries.
- [x] Centralize shared Mixed/reference allocation sizes and the raw pointer-container ABI tag.
- [x] Verify unchanged output, all supported target assemblers, focused ownership regressions, documentation and compiler build.

## Boundary

Continue from 8030c6661e8f43431d281e4de8ad33a093974e77 in refactor/memoir-extraction. Extract arrays/mixed_from_value.rs, arrays/mixed_reference.rs, exceptions/ref_cell_release.rs and exceptions/cleanup_preserve.rs into compiler-only elephc-memoir. Preserve all compiler import paths and unit-test namespaces. The host retains resource-id/subtype policy, resource inventory publication, EIR decisions, PHP type validation and exception propagation machinery. Explicit emission hooks keep resource policy out of the storage crate. The existing target ABI remains authoritative for frame-slot addresses.

## Layout

Memoir owns the canonical 24-byte Mixed cell, 16-byte managed reference payload and raw pointer-container value tag. Use those constants in the Mixed constructors and the existing managed-reference allocator without changing instruction spelling, offsets, flags, labels or ownership behavior. This is an extraction and shared-contract change, not an ABI redesign.

## Validation

Freeze output bytes and sorted internal-label fingerprints on five targets and six emission modes before moving implementation. Retain the three earlier fingerprint gates. Group baseline acquisition, extraction and focused validation under one build-queue job. Count exact matches before test execution, cross-assemble every target/mode, exercise reference sharing/replacement, resource identity, owned strings, exceptional cleanup and eval reentry, and build the compiler without new warnings. Keep source headers, function docs, assembly comments and whitespace valid. Remove task-owned tools and assembler artifacts before completion.


## Recorded validation

The baseline was captured from 8030c6661e before moving production implementation. All nine production functions were preserved, including their compiler test namespaces. Resource-id/subtype policy and weak inventory publication remain in the compiler adapter, with the frame protocol documented next to the host hooks.

The new 30 boxed-value fingerprints and the earlier 150 allocator/GC/value/storage fingerprints are unchanged. All 30 target/mode configurations cross-assemble with Clang. Twelve exact unit/integration checks passed, including resource-kind exclusions and exception-safe reference retirement on every target. The compiler build completed without warnings. Crate metadata and CLI/bridge guards confirm an ordinary rlib with zero dependencies and no runtime archive or capability.


All 22 exact Linux x86_64 executable regressions passed: reference alias/copy/return lifetimes, throwing cleanup, foreach reference retirement, scalar/null/pointer boxing, shared resource inventory and descriptor reuse, hash-context ownership, native/eval Mixed aliases, collector pins, exception chaining and direct owned-string transfer. Test builds retain the existing Magician Atomic::fetch_update deprecation and optimizer-fixture unused import in unchanged files. Header/function documentation, assembly-comment alignment and whitespace checks passed. Full-suite and foreign-target execution remain CI responsibilities.
