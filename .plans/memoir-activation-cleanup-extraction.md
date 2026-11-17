# Memoir activation cleanup extraction

- [x] Verify the clean worktree and current main ancestry before extraction.
- [x] Move the abandoned activation walk into Memoir using assembly and target ABI capabilities.
- [x] Keep owned-guard setup, protected deep cleanup and survivor selection in the compiler.
- [x] Preserve the 60 owner-record fingerprints and cross-assemble all five targets and fixture modes.
- [x] Validate host chain symbols, detach-before-callback ordering and focused native unwind regressions.
- [x] Update memory documentation, remove task-owned temporary files and commit.

## Scope

Continue from 010ec1d4ff1f3ffde7937e7a8b998a9a90fa50b2 on refactor/memoir-extraction, with main at 6ed27ebb9397e782940faf8760e3d91b75820112. Extract only the runtime activation-chain walk from exceptions/cleanup_frames.rs. The compiler emits the owned-guard and protected deep-cleanup helpers in their existing order before calling Memoir. The host supplies the chain symbol and survivor through the existing native argument convention. Records retain their previous/callback/data prefix, and each record is detached before its callback executes.

Memoir remains a dependency-free compiler rlib. This phase adds no PHP semantics, runtime libraries, handler selection, EIR ownership analysis or linking behavior. Existing assembly bytes and label bookkeeping must remain unchanged; no baseline refresh is expected.

## Validation

Run the warning-free compiler build, the existing owner-record fingerprint and cross-assembler tests, the existing detach-order test, a host-symbol contract test across all targets/modes, and the compiled compiler-only boundary guard. Run narrow native regressions covering throwing destructors, nested cleanup, argument guards and constructor unwinding. Count exact test selections, use the machine-wide build queue, audit the Memoir source/Cargo boundary, and check Rust documentation, assembly comments and whitespace.

## Results

- Warning-free compiler build and five focused compiler tests passed. The tests cover all 60 unchanged owner-record fingerprints, all 60 cross-assembler cases on the five supported targets, detach-before-callback ordering, host-supplied chain storage and the compiler-only boundary.
- Seven Linux x86_64 executable regressions passed with exact, fully qualified test selections. They cover throwing local destructors, continued child cleanup, nested catches, variadic argument guards, normal guard retirement, constructor throws and reentrant Fiber cancellation.
- All 42 instruction emission lines were moved unchanged. The eight chain accesses use the host symbol, and the new emitter requires only AssemblyEmitter and TargetAbi.
- The live Cargo/source boundary audit, Rust documentation checks, assembly-comment alignment and whitespace checks passed. No assembly baseline was updated.
- Existing test-build warnings remain limited to the Magician fetch_update deprecation and an optimizer-test unused import. The normal compiler build is warning-free.
- Task-owned runner files and assembler scratch directories were removed. The branch remains based on current main 6ed27ebb9397e782940faf8760e3d91b75820112; executable coverage on other hosts remains with CI.
