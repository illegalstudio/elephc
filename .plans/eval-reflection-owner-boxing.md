# Reflection construction ownership (#1352)

- [x] Reproduce the independent AOT and native-to-eval ownership leaks.
- [x] Retire factory references, metadata leases, duplicate names and fresh AOT boxed children.
- [x] Strengthen the historical public-name check to require zero live allocations.
- [x] Verify thirteen opaque-eval cases across PHP profiles and IR modes.
- [x] Finish the final AOT/native target matrix and standard-harness regressions.
- [ ] Rebase, run Trail checks, publish to the fork and observe exact-head CI.

Base: 40c2d2ab3cf96a54e73b9691c8e83075b7eefc72.
Worktree stays inside the canonical Elephc project. No upstream push or public comments.

## Scope

- Transfer the initial native Reflection object reference into its boxed interpreter result.
- In AOT Reflection emitters, use owned boxing for fourteen freshly materialized child reflectors and constant/default-property hashes. Shared enum values, live static properties and static callable descriptors retain borrowed boxing.
- Retire integer selector and array-get leases for method parameter targets on success and failure.
- Retire keys and fresh Reflection children after metadata array insertion; preserve independent owners returned by getters.
- Retire string-array decoder leases and reflected constant-value leases.
- Reuse the existing builtin Throwable constructor lease helper for ReflectionException and ValueError.

## Evidence and gates

- Reference compiler 487cae6 plus unrelated hash destruction fix and candidate3 leak the identical 24 blocks for an ordinary array default ReflectionParameter constructor; no spread is needed.
- Dedicated candidates 1-3 preserve failed strict heap evidence in the maintenance artifact directory.
- Candidate3 passes 77 focused Magician reflection unit tests.
- Candidate6 passes thirteen focused opaque-eval cases in fresh processes under PHP profiles 8.2 and 8.5 with IR optimization on/off: 52 executions, 20 repetitions per case, exact output and clean heaps. It includes using child metadata after its Reflection parent is unset.
- Candidate5 compiles and assembles ten complete opaque-eval program/runtime pairs across all five supported targets. Linux/iOS evidence here is assembly only.
- The existing public-name regression now requires a clean heap instead of baseline + 7 live allocations; its old assertion failed because the fixed eval result had zero live allocations.
- Candidate7 adds the exact AOT #1352 N=1/10/50 witness and an independent type/function getter ownership regression.
- Candidate8: all 72 fresh-process cases pass under profiles 8.2/8.5 with IR on/off, with exact stdout and clean heaps.
- Candidate8: thirty AOT program/runtime target pairs assemble, and all eighteen macOS/Linux native executions match PHP with clean heaps. iOS device/Simulator are assembly evidence only.
- Candidate8: all five selected standard-harness tests pass per IR mode, including the strengthened historical public-name test. The nine frozen bridge archive hashes match the source snapshot that passed all 77 Magician reflection units.
- Candidate9 strengthens the retained type/function test by allocating a differently named and typed replacement after unsetting the parent. Candidate10 applies the same allocation pressure to retained collection results; targeted replay records are preserved separately.
- Warning-free compiler build, assembly-comment alignment, source preambles and diff whitespace pass. Exact-head CI remains the publication gate.

## Separate preserved defects

- The AOT ReflectionParameter witness previously retained fourteen blocks in the native spread follow-up. Candidate8 addresses its fresh-object boxing sites and retires strings overwritten by later name initialization. The macOS N=50 witness and retained-child/collection checks leave a clean heap across the validated profiles and native targets.
- Pristine compiler 487cae6 plus matching original bridges independently reproduces two native enum Reflection metadata defects: ReflectionEnum::getName() is empty, and an unused native enum case reflected from opaque eval raises `Constant OwnerEnum::Item is not a case` before the ownership fixes. The complete source, expected PHP result and failing process output are preserved under `.maintenance/elephc-pr-maintenance-20261015/eval-reflection-owner-boxing-20261020/separate-native-enum-case-witness.txt` and candidate2/3 diagnostic JSON. The new overbroad enum-case fixture was moved out of this ownership regression; no existing test was weakened and no enum-case support is claimed.
- Native array-spread default materialization remains in its dedicated worktree. That worktree has an explicitly combined validation overlay of this branch; remove that overlay or make the dependency explicit before publication.
