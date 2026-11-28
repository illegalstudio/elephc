# Memoir activation records and consuming boxing

- [x] Verify the clean branch and current main ancestry.
- [x] Freeze complete owner-record and JSON integration output before extraction.
- [x] Commit readonly fingerprints and all-target assembler checks.
- [x] Move record publication, exact unlink and the shared record size to Memoir.
- [x] Move consuming container boxing with explicit host labels and comments.
- [x] Validate unchanged assembly, native ownership and compiler-only boundaries.
- [x] Rebase onto main bc5b97bfd4 and revalidate eval integration after the first final test run.
- [x] Update docs, remove temporary tools and commit.

## Scope

Code Journal plan: 34c79f92. Start at 534849d8c4 on current main 23314d3e1f.
Memoir owns record storage mechanics and the consume-after-boxing owner protocol.
The compiler selects cleanup/visitor callbacks, frame and stack placement, PHP
reference semantics, JSON flags, parsing and throwing policy. Host ABI forwarding
keeps target stack and register conventions authoritative in the compiler.

## Verification

Freeze output and sorted internal labels for the full owner-record adapter
inventory and complete recursive JSON decoder on five targets and six emission
modes, 60 cases total. Include caller-owned stack slots, descriptors, reference
arguments, fixed return guards, custom callbacks and exact record unlink.
Commit snapshots before moving production code. Require unchanged fingerprints
and target assembly afterward, plus focused native tests for temporary owners,
return leases, eval activation records and JSON success/error/COW cleanup.

## Original baseline verification

Jobs 2550ca21 and 2ddb37ff captured and verified all 60 fingerprints against
unchanged production code at 534849d8c4. Both complete inventories assemble for
every target and mode. Capture hooks were removed before the snapshot commit.

## Main update during validation

Main advanced to bc5b97bfd42dc248c9557c8d3c4f1f794a018cd1 while final validation
was running. Its eval early-binding changes do not touch Memoir or the native
ABI/runtime emitters. Finish the current source-stable run, commit the extraction,
then rebase and rerun the exact assembly and affected eval integration checks.

## Extraction validation

Queued job fe4b5f72 passed a warning-free compiler build, 15 exact compiler checks,
four Memoir contract tests, two compile-fail examples and 15 exact native tests.
The 60 new fingerprints and 60 existing owner-record fingerprints remain unchanged;
all associated target assembly checks passed. JSON native fixtures check both IR
optimization modes and balanced heaps, including partial errors and opaque eval.
Record integration covers return leases, throwing destructors and coroutine owners.
The source/Cargo boundary audit and its 24 Python regressions passed. Both new
modules require only AssemblyEmitter and TargetAbi, and all six new ABI operations
forward directly to existing compiler helpers. The retained JSON boundary's
instruction statements are unchanged.

## Final rebase verification

The 18 commits rebased without conflicts onto main
bc5b97bfd42dc248c9557c8d3c4f1f794a018cd1. Every range-diff entry remained equal,
and the complete Memoir, native ABI and native runtime source trees were
byte-identical across rebase. The frozen baseline commit is now b0b50778be.
No fingerprints were recaptured.

Queued job 40690a2a passed the warning-free compiler build and 11 exact follow-up
checks: eight compiler/target checks and three native integrations. These include
all 60 new assembly comparisons and target assembler cases, explicit host context,
pure eval activations, compiler-only linkage, opaque eval JSON in both IR modes,
a return lease retired before a same-frame catch, and shared coroutine owners.
Main still pointed to bc5b97bfd4 after validation.

Project README and memory-model documentation describe the final boundary. Shared
Code Journal documents af6b5fb6, 18ce8e86 and c870c1e2 now point to the Memoir
implementation on this branch while retaining their historical validation and
performance evidence. Rust module/function docs, assembly comments and whitespace
checks passed. Task-owned probe tools and assembler scratch paths were removed.
Publication and complete remote CI remain separate steps; no push or PR creation
was performed in this turn.
