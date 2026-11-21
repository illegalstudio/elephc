# Memoir value storage and runtime data extraction

- [x] Capture assembly and complete fixed-data fingerprints before this extraction.
- [x] Extract container allocation/growth, deep release, COW, cloning, owned strings and scratch allocation.
- [x] Extract buffer storage and reference-cell allocation, cloning, owner lookup and pin primitives.
- [x] Extract heap/GC/buffer state declarations without changing data order or layout.
- [x] Preserve compiler APIs, update documentation, and validate focused native/eval/ABI paths.

## Boundary

Continue from ca1c707e00319916ec9d63b978a83792c6112be7 in the existing worktree. Memoir remains a compiler-only rlib with no runtime archive or bridge. EIR lifetime analysis, frame planning, PHP type validation and class layout policy stay in the compiler. A small neutral feature descriptor and host emitter hooks carry the facts needed by type-specific memory helpers.

## Verification

Pin emitted assembly and complete fixed runtime data on all five supported targets, six emission modes and two feature configurations before moving code. Keep the first extraction fingerprints green. Preserve existing test namespaces and run exact focused COW, deep cleanup, references, buffer and string-ownership regressions through the machine-wide queue. Cross-assemble affected helpers for every supported target, build the compiler warning-free, and check source/header/comment hygiene.


## Recorded validation

- Preserved all 87 production functions from the 29 moved leaf files, plus the eight reference-cell primitives extracted from the compiler coordinator.
- Passed 29 exact unit/integration checks, including the existing 30-case allocator/GC fingerprints and the new 60-case value-helper/fixed-data fingerprints.
- Cross-assembled helpers and complete fixed runtime data in all 60 target/mode/feature configurations with Clang.
- Passed 25 exact Linux x86_64 codegen regressions covering array/hash COW, reference-aware cloning, buffer release/overflow/stale aliases, native destructor exceptions, allocator metrics, eval metrics/buffers and transient string ownership (including optimization disabled).
- Built the compiler with `cargo build -p elephc --bin elephc`, without warnings.
- Confirmed Memoir has zero dependencies, emits only an rlib and has no bridge registration or `--with-memoir` capability.
- Checked module preambles, function documentation and assembly-comment alignment. Full-suite and foreign-target execution remain CI responsibilities.
