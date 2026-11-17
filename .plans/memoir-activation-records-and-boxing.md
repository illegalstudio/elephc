# Memoir activation records and consuming boxing

- [x] Verify the clean branch and current main ancestry.
- [x] Freeze complete owner-record and JSON integration output before extraction.
- [x] Commit readonly fingerprints and all-target assembler checks.
- [ ] Move record publication, exact unlink and the shared record size to Memoir.
- [ ] Move consuming container boxing with explicit host labels and comments.
- [ ] Validate unchanged assembly, native ownership and compiler-only boundaries.
- [ ] Update docs, remove temporary tools and commit.

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
