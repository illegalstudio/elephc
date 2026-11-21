# Memoir reference storage extraction

- [x] Rebase the clean branch onto current main.
- [x] Compare complete fixed runtime data against the original main emitter and refresh affected fixtures.
- [x] Freeze weak-cache and hash-reference assembly before changing production emitters.
- [x] Extract weak-cache publication, borrowed reads and retirement with explicit host class identity.
- [x] Extract hash-entry promotion and borrowed dereference, including inline register views.
- [x] Validate exact assembly, all target assemblers, focused ownership regressions and compiler-only gates.
- [x] Audit the remaining boundary, update documentation, remove temporary artifacts and commit.

## Scope

Code Journal plan: `61c8c67f`. The branch was rebased from
`1c67c7475d691751bdfd262f3e0a9043acf52a50` onto main
`23314d3e1fdb1ec72b5a915a7753dc72473e7591` without conflicts.
The first commit's range-diff changes only surrounding target context; the other
thirteen commits retain identical patches. Upstream added ctype deprecation data,
which must be verified against the independent original main emitter before the
five complete-data fixture groups are updated.

Memoir remains a dependency-free compiler rlib. The compiler supplies weak-wrapper
class identity and keeps PHP method validation. Hash iterator cursor rules,
successor anchors and foreach semantics remain compiler-owned. Promotion transfers
the existing entry owner into a reference cell after separating a shared Mixed
box; dereference returns a borrow without allocation or refcount changes.

## Verification

Capture the complete weak-reference emitter and complete hash-reference integration
before moving code, with output and sorted internal labels. Include two inline
register arrangements. Five targets and six output modes produce 60 cases.
Commit readonly snapshots before extraction and require unchanged fingerprints
afterward. Cross-assemble all cases and run exact native tests for weak identity,
cycles, Closure observers, wrapper recreation, entry COW, growth, escaped aliases
and borrowed value readers. Run the compiler-only Cargo/source audit, its focused
regressions, a warning-free compiler build and source hygiene checks. Every build,
link and executable test runs through the machine-wide queue.

## Original reference verification

The independent main emitter matched complete fixed data on every supported
target at three heap sizes. All 270 affected fixture rows were refreshed only
after that byte comparison. The 60 new reference fingerprints and all 330
relevant target assembly cases passed before production extraction. The capture
hooks and independent probe module were removed before committing.

## Boundary audit

The two extracted modules require only AssemblyEmitter and TargetAbi. No semantic
compiler type or MemoryContext callback is needed. Hash reference tags use the
existing ValueTag contract. The runtime-cache source walker recursively includes
both new modules.

Reviewed retained integration includes parser_owners and reference_registry under
unserialize, JSON construction ownership, eval owner frames, rejected container
writes, and native property/source metadata. These select parser failure policy,
physical PHP layouts, descriptor authentication or bridge activation behavior.
Their storage operations already use the shared memory helpers; moving those
policies is outside this extraction. Hash cursor resync and successor selection
remain next to their compiler integration.

No pull request exists for this branch. Publication and the complete remote CI
matrix remain separate steps; local validation covers native Linux x86_64 and
assembly for every supported target.

## Results

- Snapshot commit b447f7f969 froze both reference inventories before production
  changes and refreshed the five data groups only after independent byte equality
  with the original main emitter.
- The two Memoir modules preserve 108 weak-cache and 136 hash-reference instruction
  emission statements. Both new snapshot files are unchanged after extraction.
- Final queued job df7f65a1 passed: warning-free compiler build, 29 exact compiler
  checks, four Memoir contract tests, two compile-fail examples and 13 exact native
  ownership tests. All 480 frozen fingerprints remain identical. Cross-assembler
  checks cover all five supported targets and every emission mode.
- Native coverage includes weak-wrapper reuse and recreation, cycles, Closure
  observers, WeakMap heap accounting, coroutine cancellation, reference COW,
  growing tables, escaped aliases, successor resync and pop/shift value readers.
- Cargo/source boundary audit, its 24 Python tests, all 20 PR classifier tests,
  Rust preambles/function docs, assembly comments and whitespace checks passed.
- Original-main probes, capture hooks, runner scripts and assembler scratch paths
  were removed. Main remained at 23314d3e1f. Remote CI and publication are pending;
  this turn neither opened a PR nor pushed the branch.
