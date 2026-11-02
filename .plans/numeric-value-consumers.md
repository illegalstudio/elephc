# Numeric value consumers

- [x] Inspect the typed addition and multiplication reduction IR on main.
- [x] Admit only closed numeric producer inputs and checked scalar return observations.
- [x] Preserve single-local storage, same-block observations and per-slot borrowed-read invalidation.
- [x] Verify typed-reduction allocation bounds, PHP overflow continuation and typed return failures.
- [x] Verify aliases, delayed reads, nonnumeric refusals and every supported emitter.
- [x] Reuse dead transient cells and preserve the existing Fiber recursion regression.
- [ ] Measure pinned main, the candidate and real PHP on identical runtime inputs.
- [ ] Complete independent review and CI before requesting maintainer review.

The closed numeric-frame proof currently rejects a checked product used by another
numeric operation and a numeric local observed by a declared integer return check.
Both consumers copy numeric payloads. This needlessly restores per-operation heap
allocation in ordinary typed reductions and integer index expressions.

No range assumption removes the overflow branch. Existing integer and float tags,
the checked arithmetic emitters and the return-boundary TypeError behavior stay
authoritative. References, eval, suspension, unknown consumers and escaping results
retain heap-backed storage.

Published `OwnedTemp` and `HiddenTemp` operand temporaries may hold a selected
numeric cell with one same-block store, balanced owner records, final retirement
and scalar observations.
The ordinary exception cleanup remains authoritative and ignores frame addresses.

Transient storage follows same-block payload lifetimes through retains/borrows.
All local owners keep dedicated cells. Numeric consumers read both payloads before
writing, so a result may reuse a dead input's cell. The original Fiber stack guard
and recursion test remain unchanged.

Buffer index tests use weighted checksums and explicitly free their buffers. Plain
scalar writes isolate index arithmetic; element `++` remains a separate fallback
with a boxed seed, increment and copy. This change does not remove that fallback.
