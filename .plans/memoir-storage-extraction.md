# Memoir growable storage and ownership guards

- [x] Freeze assembly and internal-label fingerprints before moving the four selected blocks.
- [x] Extract growable string buffers, consuming string append, callable descriptor release and hash ownership guards.
- [x] Preserve compiler module paths, shared descriptor constants and existing integration tests.
- [x] Verify frozen output and cross-assembly across all supported targets and emission modes.
- [x] Run focused executable ownership regressions, update documentation and commit.

## Scope and boundary

Continue from e96cc646e3847d107ba36af04273d8be1298ff57 in refactor/memoir-extraction. Move only the four blocks approved by the user: strbuf, str_append, callable descriptor release and hash_write_guard. Keep object-handle identity, PHP exception handling, EIR ownership decisions and mbstring metadata policy in the compiler. Memoir remains a dependency-free compiler-only rlib with no runtime archive, bridge or CLI capability. Preserve emitted helper labels, instructions, metadata ordering and existing compiler test namespaces.

## Validation

Freeze helpers on five targets, six emission modes and mbstring disabled/enabled before extraction. The digest includes sorted internal-label bookkeeping. Cross-assemble all configurations, retain both earlier extraction fingerprint gates, count exact test matches before running focused units and native ownership regressions through the build queue, and build the compiler without new warnings. Check module/function documentation, assembly-comment alignment and whitespace. Remove task-owned tools and assembly artifacts before completion.

## Validation recorded

Preserved all 13 production functions, including the append dispatcher following the original external test-module declaration. Eleven exact unit/integration tests passed, including the existing 90 fingerprints, the new 60 storage fingerprints, all 60 cross-assembly configurations, callable reference-capture cleanup and the append ABI/copy-volume harness. The compiler Emitter interface required no additional methods.


Eighteen exact Linux x86_64 executable regressions passed, covering consuming append (sharing, overlap, NUL bytes, reference captures and mbstring origins), large/nested JSON and serialization streams, throwing cleanup, callable receiver/capture lifetimes, eval closure captures and reentrant/deferred hash-owner release. The normal compiler build completed without warnings. Test builds retain only the existing Magician Atomic::fetch_update deprecation and unused optimizer-fixture import in unchanged files.

The former arrays/deep_cleanup module is now test-only because callable descriptor retirement was its last production consumer; the implementation already lives in Memoir. Crate metadata still declares only an rlib with zero dependencies. Compiler-only CLI/bridge guards passed. Module/function documentation, assembly-comment alignment and staged whitespace checks passed. Full-suite and foreign-target execution remain CI responsibilities.
