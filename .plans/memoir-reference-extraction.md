# Memoir reference storage extraction

- [x] Rebase the clean branch onto current main.
- [x] Compare complete fixed runtime data against the original main emitter and refresh affected fixtures.
- [x] Freeze weak-cache and hash-reference assembly before changing production emitters.
- [ ] Extract weak-cache publication, borrowed reads and retirement with explicit host class identity.
- [ ] Extract hash-entry promotion and borrowed dereference, including inline register views.
- [ ] Validate exact assembly, all target assemblers, focused ownership regressions and compiler-only gates.
- [ ] Audit the remaining boundary, update documentation, remove temporary artifacts and commit.

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
