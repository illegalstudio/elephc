# Native numeric cells

- [x] Prove all consumers and complete local storage before reserving a frame cell.
- [x] Emit checked add, subtract and multiply with PHP float promotion on all five targets.
- [x] Run focused PHP, ownership, IR-mode and target-emission regressions.
- [ ] Measure matched release binaries and actual PHP, with allocation counters and raw samples.
- [ ] Complete review and publish the focused arithmetic change.

The first change removes transient heap allocation without narrowing the PHP numeric domain.
A selected checked result uses a distinct 24-byte frame cell with the ordinary Mixed tag,
low payload and high payload. Signed overflow converts both original integers to double.
It never treats an overflowing value as an unconditionally fitting integer.

Only same-block numeric casts, retains, releases and one ordinary PHP local may consume a
selected pointer. Every access to that local is checked, including later source assignments.
A borrowed Mixed read must be converted before any checked producer can write a cell again.
Parameter, global, static, reference and eval storage, suspension, unknown consumers, second
destinations and escaping returns retain heap allocation. A scalar load copies the payload
before its SSA consumers run. Abstract ownership operations retain their EIR meaning, while
proven frame cells need no runtime retain or release. Numeric heap seeds still need one
balanced release. The existing validating Mixed decref ignores frame addresses; the numeric
local proof also permits direct cleanup without a protected PHP destructor boundary.

This representation change does not extend closure call authorization or change argument
types. Closure accumulator and callable-return costs require their own measured follow-up.
The existing range analysis and explicit integer-cast transformations remain authoritative.
