# Stable keys for eval array-element references

- [x] Preserve the original failing native fixtures and PHP output oracles.
- [x] Replace persistent native key handles with normalized Rust-owned key values.
- [x] Audit reference reads, writes, comparisons, cloning and captured targets.
- [x] Add semantic regressions for rebinding, copies, nesting and mutating calls.
- [x] Pass six new regressions and 108 focused interpreter tests.
- [x] Validate the current compiler and bridge in both IR modes with clean heaps.
- [x] Run Linux target checks, hygiene and Trail reconciliation; record the pre-existing iOS blocker.
- [ ] Rebase, squash and publish the dedicated fork branch.

Base: `cb407fe968e21083055914edc243131464dde234`.

The previous target metadata retained the `RuntimeCellHandle` created while
evaluating an element offset. Alias-map cloning copied that handle without a
matching owner protocol, and retirement discarded it without releasing its
native allocation. Borrowed index variables could also be rebound before a
later alias access, leaving a stale key cell.

`PersistentArrayElement` carries an `EvalArrayReferenceKey` integer or byte vector.
Array argument lowering snapshots the key while its cell is live, preserves the
read result independently, and retires key and intermediate receiver leases on
all exits. The existing temporary target variants remain available to scoped
reference adapters. The persistent readers, equality checks and write-back paths
accept the new representation, including the protected native reader that defers
intermediate-owner cleanup until its context borrow has ended.

Evidence stays under
`.claude/worktrees/.maintenance/elephc-pr-maintenance-20261015/reference-key-lease/`.
`oracles.json` records the nine original PHP programs and their failing old native
results. The independent `pr1568` worktree retains the outstanding review fixes;
this pre-existing reference defect is corrected on its own branch.

The unrelated associative destructor-order defect and pre-existing iOS pcntl
compilation failure are separate work. They are not acceptance evidence for this
reference-key change and must not be hidden by weaker output or heap assertions.

Final bounded evidence before publication: 108 focused interpreter tests; nine
native reference cases in each IR mode with exact PHP output and clean heaps;
all nine PHP 8.2/8.5 oracle outputs agree; Linux ARM64 and x86_64 Magician checks
pass. The registered integration-test source also compiles against the frozen
compiler library. Its two registered native tests also pass (native-test-probe-run.log).

Independent sort/implode/array_keys iterator leaks reproduce without references.
Their complete original programs and heap traces remain in residual-isolation.json
and INDEPENDENT-BUILTIN-LEAKS.md in the maintenance evidence. The reference-key
witness keeps exact binary-key access and clean-heap assertions without routing
its output through the separately leaking temporary array_keys result. A user
by-reference function is the native call-target witness; mutating-builtin target
semantics remain covered by the interpreter unit. None of the original seven
PR #1568 review fixtures were weakened or removed.
