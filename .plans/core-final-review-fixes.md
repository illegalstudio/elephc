# Core ownership and callable follow-ups

- [x] Ship PR #893's managed reference cells, call-operand roots, returned leases, and exception-safe cleanup.
- [x] Consolidate the historical Core review and property-reference cleanup logs.
- [x] Ship PR #1674's compatible native enum interface reference returns and exception-safe reference rebinding.
- [ ] Define the missing ownership-transfer contract for unresolved by-reference callable results before extending reference assignment.
- [ ] Requalify the remaining descriptor/interface reference-return reports against current source and their Code Journal tasks.
- [ ] Add focused normal/exceptional and supported-target regressions for each accepted follow-up.

## Status and provenance

The old `core-owned-reference-cell-cleanup.md` and this file described work on
`feat/core-align`. PR [#893](https://github.com/illegalstudio/elephc/pull/893)
merged as `fcec655d8d0e` on 2026-09-22; its check rollup is successful. Old
claims that the PR remains unmerged or still awaits its original CI are stale.
This is a remaining-work backlog, not a new claim that all callable reference
forms have PHP parity.

Full investigation history remains in Git at `54c5c3332e`, including both
original filenames. Current contracts live in
[the memory model](../docs/internals/memory-model.md),
[functions](../docs/php/functions.md), and
[the EIR reference](../docs/internals/the-ir.md).

## Shipped safety contract

Managed cells keep their own heap identity and two-word payload. The collector
walks `object -> cell -> value`; external aliases keep the cell alive. Clone
paths separate singleton cells while preserving real PHP reference sets.

A callee publishes its returned cell lease before temporary receivers, arguments
or its epilogue can release the pointee. The caller snapshots the selected
pointer separately from that lease. Ordinary by-value use copies the pointee
and then retires the lease. An ownerless address is admitted only through the
exact bounded-borrow contract, never by interpreting a raw value as a cell.

Call arguments, partial CUF/CUFA containers, callback descriptors and results
must be rooted before a later argument, conversion or destructor can throw.
Cleanup clears published slots before release, preserves an earlier pending
exception, and retires records in LIFO order. Fallthrough `finally` rebinding
must not change the pointer already selected for return.

Native enum interface dispatch now transfers the selected method's actual reference
when its payload layout matches the interface result. A selected by-value method
creates an independent result cell instead. Incompatible reference layouts and
runtime-selected descriptor calls keep their explicit unsupported diagnostics.
These shipped paths do not close the historical interface reports below without
checking their exact reproducers.

## Remaining boundaries

| Work | Current evidence / owner |
|---|---|
| Dynamically selected by-reference call assignment | `src/ir_lower/expr/property_access.rs::lower_ref_assign_call` explicitly refuses a call that did not adopt a staged cell. Preserve that diagnostic until descriptor dispatch can transfer an owned reference. |
| Borrowed/interior addresses and incompatible result payloads | `src/ir_lower/stmt/control_exit.rs` keeps explicit refusals and bounded-borrow checks. Do not remove them merely to accept a new callable form. |
| Descriptor return-error/replacement ownership | Code Journal task `1410be72`; refresh its reproducer before proposing a change. |
| Repeated interface reference-return retention | Code Journal task `e4e9fca4`; retain the distinction between a historical observation and a current reproduced defect. |
| ArrayAccess reference-return override adaptation | Code Journal task `296951a6`; an ABI adaptation is separate from relaxing ownership validation. |

The prior unfinished IteratorAggregate/CUFA and operand-cleanup drafts have
structural counterparts in `src/ir_lower/tests/callable_flow_safety.rs`,
`iterator_aggregate_owners.rs`, and `callable_operand_owners.rs`. Returned-cell,
finally and bounded-borrow cases live in `reference_return_boundaries.rs` and
`tests/codegen/runtime_gc/reference_cell_owners.rs`. Their presence and merged
CI replace the obsolete publication checklist; it does not close unrelated
reports without checking their exact source.

## Follow-up acceptance

For each accepted change cover temporary receivers, aliases, cloning, cycles,
normal values versus reference results, same-frame catches, throwing cleanup,
and both IR optimization settings. Inspect target-aware EIR/emission on all
five supported targets. Do not raise timeout budgets or weaken heap assertions
to turn a report green. No tests were executed during this consolidation.
