# Memoir hydration ownership extraction

- [x] Verify the clean branch and current main ancestry.
- [x] Capture 30 original unserialize assembly fingerprints before changing production emitters.
- [x] Commit the frozen fingerprint and cross-assembler tests against the original compiler code.
- [x] Move stored-property completion, previous-owner release and box reuse into Memoir.
- [x] Keep the uninitialized marker, scope deferral and replaced-payload callback explicit in the compiler adapter.
- [x] Validate unchanged assembly, all five targets, host configuration and narrow native ownership regressions.
- [x] Update documentation, remove task-owned temporary artifacts and commit.

## Boundary

Continue from b1a089c8272c198518c579d42f7f7f7652c5acb8 on refactor/memoir-extraction, based on main 6ed27ebb9397e782940faf8760e3d91b75820112. The compiler keeps wire parsing, property lookup, type validation, numeric coercion, duplicate-key selection, reference registry and context completion. Memoir receives only the marker and callback symbols needed for the existing storage-owner protocol. It remains an ordinary compiler rlib and adds no generated-program Rust linkage.

## Validation

Freeze the complete unserialize emitter output and sorted internal labels, without the unrelated fixed runtime-data catalog, for five targets and six emitter modes. Capture before extraction, then require identical fingerprints and cross-assembler results after extraction. Keep existing property-owner contract tests and add a host marker/callback test. Run a warning-free compiler build, the compiled compiler-only boundary guard, the live source/Cargo audit and exactly selected native cases covering overwritten defaults, Mixed aliases, duplicate replacements, retained back-references, nested scopes and cleanup throws. Queue every compilation, linker and executable test operation through build-lock.

## Results

- Snapshot commit 5a47708b3150759450ed347ec0ed9035a36bd5f3 captured all 30 complete-parser fingerprints from the original production emitter before extraction. The readonly fingerprint and target assembly tests passed before production code changed.
- The extraction moves 129 instruction emission lines unchanged into hydration_owners. Its Context contains only the host marker and two callback symbols, and emission requires only AssemblyEmitter and TargetAbi.
- After extraction, all 30 fingerprints remain identical and all 30 cases assemble on the five supported targets. No baseline was refreshed.
- Fourteen exactly selected tests passed: six compiler/assembly checks and eight Linux x86_64 ownership regressions. They cover overwritten defaults, Mixed reference slots, duplicate object/container replacement, later back-references, nested scopes and cleanup throws. The host-context test covers every target and output mode.
- Compiler build is warning-free. The compiled compiler-only guard, live source/Cargo audit, Rust documentation, assembly-comment alignment and whitespace checks passed. Existing test-build warnings remain unrelated.
- Task-owned capture sources, runner files and assembler scratch directories were removed. Main remained at 6ed27ebb9397e782940faf8760e3d91b75820112; other-host executable coverage remains with CI.
