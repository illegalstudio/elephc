# Memoir compiler boundary and API contracts consolidation

- [x] Add a tested dependency-free boundary audit and wire it into host CI.
- [x] Split the 58 host services into assembly output, target ABI and memory context capabilities.
- [x] Introduce distinct heap/value tags and record/frame offsets with validated guard layouts.
- [x] Replace positional guard/scoped-owner facts with explicit contracts at compiler adapters.
- [x] Keep all 390 frozen fingerprints unchanged and cross-assemble their target cases.
- [x] Run focused contract, boundary and executable checks; update documentation and commit.

## Boundary

Continue from 472ffd25338190e5e52f60eb09cf523d2e86a72e in refactor/memoir-extraction. This phase changes compiler APIs and automated gates, not emitted runtime semantics. Memoir remains a dependency-free rlib consumed in production only by the compiler. The Cargo metadata audit checks every target-specific dependency and workspace production consumer, including aliases and transitive local paths. A source audit ignores comments/literals while rejecting PHP/EIR coupling and native-export/link attributes. The compiled bridge-catalog assertion remains authoritative for generated-program linkage.

## Contracts

Emitter remains the composed facade. AssemblyEmitter, TargetAbi and MemoryContext expose separate capabilities; ABI forwarding functions require only their actual capability. Compiler implementations retain all 58 method bodies. HeapKind and ValueTag are disjoint; RecordOffset and FrameOffset describe pointer-sized storage in different bases. GuardLayout validates the owner after the activation prefix. ScopedOwnerState names head and depth symbols. Compiler type policy and target ABI remain authoritative.

## Validation

Test metadata failures for native crate types, dependencies, optional compiler linkage, aliases, target-specific and transitive runtime consumers. Test source tokenization against comments, raw/byte strings, characters, lifetimes, nested comments, forbidden types and export attributes. Add compile-fail contract examples and invalid-layout tests. Run standalone Memoir tests/doc tests, all frozen compiler fingerprints and existing cross-assembler tests, plus narrow runtime regressions under the machine-wide build queue. Validate workflow syntax and audit coverage on host jobs. Remove task-owned temporary artifacts before completion.

## Results

- Warning-free compiler build through build-lock job 76f7fd37.
- 24 boundary-audit regression tests and the live Cargo/source audit passed.
- 34 exactly selected Rust tests and two compile-fail doctests passed. These include all eight frozen fingerprint groups, all eight cross-assembler groups, six compiler boundary/cleanup checks, four invalid-contract tests and eight Linux x86_64 executable regressions.
- All 390 frozen assembly cases remain unchanged and assemble for the five supported targets and their fixture modes. No baselines were recaptured.
- Workflow syntax and both Memoir gates were checked on all three host builders. Rust preambles, function docblocks, assembly comment alignment and whitespace checks passed.
- Test builds retain the pre-existing Magician fetch_update deprecation and optimizer-test unused import warnings. The normal compiler build is warning-free.
- Task-owned runner files and assembler scratch directories were removed. Native execution on the other hosts remains covered by CI.
