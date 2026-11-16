# Memoir string and interoperability storage extraction

- [x] Capture existing string, C-string, zval and capture-buffer assembly before extraction.
- [x] Extract byte-string concatenation and C-string allocation/copy helpers.
- [x] Extract zend_string allocation and recursive zval/array retirement.
- [x] Extract capture-buffer growth, finalization, abort and fixed storage declarations.
- [x] Keep output routing, capture-mode policy and zval pack/unpack in compiler adapters.
- [x] Validate fingerprints, target assembly, focused executable regressions, documentation and build.

## Boundary

Continue from 9f41af9a4941cd37fa5e26265adc24a917bd9b68 in refactor/memoir-extraction. Memoir remains a compiler-only rlib without dependencies, bridge registration or target-program linkage. Preserve compiler entry points and existing test namespaces. The capture storage emitter receives a compiler callback that resets routing mode after clearing storage; it does not choose the destination of output. PHP formatting, zval pack/unpack and builtin validation stay in the compiler.

## Validation

Freeze instruction output, sorted internal labels and complete fixed runtime data on five targets, six emission modes and two string scratch capacities. Retain the earlier 210 fingerprints. Cross-assemble every new target/mode/capacity case. Count exact tests before running narrowly selected concat ownership/overflow, C-string null/byte-copy, recursive zval release, print_r capture growth/abort and output-routing regressions. Group compilation and executable tests under the machine-wide queue. Verify source headers, function documentation, assembly-comment alignment and whitespace. Remove task-owned temporary tools and assembler objects before completion.

## Recorded validation

The 60 fingerprints were captured from 9f41af9a4941cd37fa5e26265adc24a917bd9b68 before moving production code. All 18 moved function bodies match after the declared host-interface substitutions and the capture-mode reset callback. Both compiler output-routing functions remain byte-for-byte unchanged. Capture storage declarations retain the original offset/pointer/capacity ordering immediately after the compiler-owned mode flag.

Source preambles, function documentation and assembly comments pass for all 18 touched Rust files. Metadata confirms a compiler-only rlib with zero dependencies and no runtime archive. Existing compiler test namespaces and all zval pack/unpack code are retained.

The compiler builds without warnings. All 270 frozen fingerprints pass, including the earlier 210 cases and the new 60 target/mode/capacity cases. All 60 new cases cross-assemble with Clang on macOS, iOS device/Simulator, Linux AArch64 and Linux x86_64. All 14 selected unit checks and 24 Linux x86_64 executable regressions passed, covering concat heap fallback and later string work, C-string argument/return ownership, recursive zval release, repeated large capture growth, nested output buffering and exceptional capture cleanup. Test builds retain the existing Magician Atomic::fetch_update deprecation and optimizer-fixture unused import in unchanged files. Full-suite and foreign-target execution remain CI responsibilities.
