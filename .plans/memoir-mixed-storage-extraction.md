# Memoir Mixed storage and cleanup proofs extraction

- [x] Capture original assembly for array unboxing, Mixed reference storage and cleanup gates.
- [x] Extract Mixed-array slot conversion with unchanged ownership and mismatch behavior.
- [x] Extract Mixed reference retain, wrapper promotion and writable child-slot selection.
- [x] Extract scalar/empty-array cleanup eligibility proofs while keeping PHP tag policy in the compiler.
- [x] Keep native type-source association, exception handlers and diagnostics compiler-owned.
- [x] Validate frozen output, target assemblers, focused executable regressions, documentation and build.

## Boundary

Continue from 775d2eb9bb5c5bdc2c6b0d244796323067705137 in refactor/memoir-extraction. Memoir stays a dependency-free compiler-only rlib without runtime archives or bridge registration. Retain compiler entry points and test namespaces. The host supplies scalar tags with no child ownership, the uninitialized-property sentinel and an optional post-promotion type-source callback. Native type constraints, diagnostic selection and protected exception machinery remain in the compiler.

## Validation

Capture bytes, sorted internal labels and full fixed data on five targets, six emission modes and both native-reference validation settings before moving code. Explicitly emit the inline writable-slot selector so it is covered independently of other runtime consumers. Retain all earlier 270 fingerprints and cross-assemble every new case. Compare all moved function bodies after the documented host substitutions. Count exact tests before executing focused array writeback, persistent alias, type-source association and scalar/empty-array cleanup regressions through the shared build queue. Check source documentation, assembly-comment alignment, metadata and whitespace. Remove task-owned temporary tools and assembler artifacts before completion.

## Recorded validation

The 60 new fingerprints were captured from 775d2eb9bb5c5bdc2c6b0d244796323067705137 before moving production code. All 11 moved function bodies match after the declared host substitutions. Scalar tags remain selected from PhpType in the compiler adapter; native type-source association is passed as an optional callback. Exception handler installation, source metadata lifecycle and diagnostics remain unchanged compiler code. The writable-slot selector is explicitly included in every new fingerprint and assembler case.

Documentation and assembly-comment checks pass for all 14 touched Rust files. Crate metadata confirms a dependency-free rlib and no target-program archive. The original compiler test namespaces are retained, including a test-only compatibility export for the empty-array candidate label.

All 330 frozen fingerprints pass, including the new 60 target/mode/native-validation cases. All 60 new cases cross-assemble with Clang on macOS, iOS device/Simulator and both Linux architectures. All 16 selected unit checks and 20 Linux x86_64 executable regressions passed. Coverage includes nested/raw/string/callable slot conversion, mismatch diagnostics, persistent property/constructor/static aliases, collector pins, pending exceptions, throwing siblings and scalar/empty-array destruction under both optimizer modes. After restricting the unused compiler label compatibility export to tests, a separate final compiler build passed without warnings. Pre-existing Magician Atomic::fetch_update and optimizer-fixture warnings remain outside this change. Full-suite and foreign-target execution remain CI responsibilities.
