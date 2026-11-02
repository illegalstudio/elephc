# Bounded Mixed counters

- [x] Prove every source, consumer and update of a boxed local before changing representation.
- [x] Commit a scalar trial only after SSA promotion and range analysis remove every overflow check.
- [x] Cover inclusive bounds, late type changes, aliases, call-site separation and both IR modes.
- [x] Validate heap ownership, supported target emission and matched native/PHP benchmarks.
- [ ] Complete independent review and publish the focused change.

The optimization consumes existing direct calls and does not change closure call eligibility.
Each candidate ordinary local must have integer seeds and only transparent numeric ownership,
integer comparisons, direct calls and increment/decrement consumers. References,
implicit eval scope access, parameters, static/global storage, suspension and exceptional CFGs
retain the original representation. The original function is untouched when a trial fails.

A private function clone rewrites a candidate to integer storage, promotes it to SSA, and asks
IntegerRange to prove every checked update safe. A strict integer bound may make an increment
safe even when its runtime limit is PHP_INT_MAX. Inclusive bounds and unrelated conditions do
not justify dropping a potential overflow. Calls are admitted only at physical by-value Mixed parameter positions from module metadata.
They retain the original callee, declared signature and result representation; existing ABI argument materialization handles an integer operand.
The pass does not infer a type from the first loop iteration or authorize another call site.
