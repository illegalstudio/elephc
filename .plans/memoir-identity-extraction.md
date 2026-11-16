# Memoir identity and temporary owners extraction

- [x] Freeze the existing assembly and fixed-data output on all five targets and six emission modes.
- [x] Extract Mixed unboxing, temporary-owner retirement and boxed Throwable transfers.
- [x] Extract reference-parameter acquisition and original-identity pinning.
- [x] Extract the object-handle pool and side-table declarations, keeping PHP hash formatting in the compiler.
- [x] Extract fiber stack mapping/free emission, keeping platform mapping flags in the compiler.
- [x] Validate unchanged output, target assembly, focused executable regressions, documentation and build.

## Boundary

Continue from 58ee974dc47d68a8984b096c860a7fc6465434e9 in refactor/memoir-extraction. Memoir remains an ordinary compiler-only rlib without dependencies, bridge registration or target-program linkage. Preserve compiler entry points and existing test namespaces. PHP validation, object hash formatting, platform mapping policy, EIR decisions and runtime orchestration remain compiler responsibilities.

## Validation

Capture the baseline before moving production code. Compare instruction output, sorted internal labels and all fixed runtime data across executable, PIC, library, extension and dead-strip modes. Retain existing frozen gates. Cross-assemble all supported targets. Count exact tests before running focused owner-transfer, reference, pinning, object identity and fiber-stack regressions. Run all compilation and executable tests through the shared build queue. Remove task-owned tools and test artifacts before completion.

## Recorded validation

The 30 new fingerprints were captured from 58ee974dc47d68a8984b096c860a7fc6465434e9 before moving production functions. All 25 moved function bodies match the originals after the explicit host-interface substitutions. The PHP hash formatter and platform mapping flag selector remain in compiler modules. The only production additions are the host mapping fact and neutral fixed-data delegation; generated helper order and side-table placement are preserved.

The compiler builds without warnings. All 210 frozen fingerprints pass, including the earlier allocator/GC, value, storage and boxed-value gates. All 30 new target/mode cases cross-assemble with Clang, including macOS, iOS device/Simulator and both Linux architectures. The 13 selected unit checks passed. Crate metadata confirms a dependency-free rlib and no target-program static archive. Source preambles, function documentation, assembly-comment alignment and whitespace checks passed.

All 18 exact Linux x86_64 executable regressions passed: native/eval Throwable transfers, temporary operand cleanup on normal and throwing paths, native reference acquisition and handoff, original identity pinning across frames, stable object IDs/hash formatting, Fiber stack release and suspend/resume, owner transfers and stack-floor restoration. Full-suite and foreign-target execution remain CI responsibilities. Test builds retain only the existing Magician Atomic::fetch_update deprecation and optimizer-fixture unused import in unchanged files.
