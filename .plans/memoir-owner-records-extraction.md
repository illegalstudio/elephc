# Memoir static bindings and owner records extraction

- [x] Capture original static-binding, ownership-guard and scoped-owner assembly.
- [x] Extract static binding payload, promotion, retirement and canonical storage sizes.
- [x] Extract ownership-guard link/unlink and shared take-before-release primitives.
- [x] Extract temporary owner-node allocation and scope-prefix detachment.
- [x] Keep weak source unbinding, constructor policy, parser completion and exception handlers in the compiler.
- [x] Validate fingerprints, target assembly, focused executable regressions, documentation and build.

## Boundary

Continue from 2b9042bfa4ccc51d5c03bef28101641d7e3e5186 in refactor/memoir-extraction. Memoir remains a dependency-free compiler-only rlib with no target-program archive or bridge registration. The compiler supplies the static weak-source unbind callback, uninitialized sentinel, guard owner/argument offsets and scoped-owner state symbols. Failed-constructor destructor policy, native type-source metadata, parser completion, wire format, diagnostic selection and exception-handler setup remain compiler-owned.

## Validation

Capture output, sorted internal labels and full fixed data on all five targets, six emission modes and both native-reference validation settings before extraction. Include an explicit static-binding read probe and declaration sized through the original canonical binding-size export. A focused test hook emits temporary owner records without freezing the whole unserialize parser. Retain all earlier 330 fingerprints and cross-assemble every new case. Verify moved bodies after explicit host substitutions and shared guard-take factoring. Count exact tests before running static reference ownership, throwing constructor/argument cleanup and nested/reentrant unserialize regressions through the machine-wide queue. Check source documentation, assembly comments, crate metadata and whitespace. Remove task-owned tools and assembler artifacts before completion.

## Recorded validation

The 60 new fingerprints were captured from 2b9042bfa4ccc51d5c03bef28101641d7e3e5186 before moving production code. Static storage owns the canonical layout offset and binding byte count. All 11 original moved bodies match after explicit host substitutions and the shared guard-take factoring. Static readers, failed-constructor policy and parser completion are unchanged. Weak-source unbinding stays in a compiler callback, guard geometry is supplied by the compiler ABI constants, and scoped nodes receive their state symbols from the compiler.

The compiler builds without warnings. All 390 frozen fingerprints pass, including all earlier 330 cases. All 60 new target/mode/native-validation cases cross-assemble with Clang. The 15 selected unit checks passed, including all-target static-binding, constructor and nested temporary-owner invariants. Source documentation and assembly comments pass for all 12 touched Rust files. Metadata confirms a dependency-free compiler rlib and no runtime archive.

All 20 exact Linux x86_64 executable regressions passed: static binding retirement/readers/writers, publication before destructor reentry, normal and throwing argument/constructor guards, nested owner scopes, later back-references, partial parse cleanup, destructor reentry and throwing hydration cleanup. The 36 final exact checks passed after the original baseline capture. Test builds retain the existing Magician Atomic::fetch_update deprecation and optimizer-fixture unused import in unchanged files. Full-suite and foreign-target execution remain CI responsibilities. Task-owned temporary tools and assembler artifacts were removed before completion.
