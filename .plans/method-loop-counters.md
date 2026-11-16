# Method loop counters

- [x] Capture the nested method-loop IR on pinned main before changing production code.
- [x] Admit exact resolved by-value argument positions without weakening shared alias analysis.
- [x] Canonicalize unchanged scalar join parameters only inside discarded storage trials.
- [x] Verify both counters narrow while the independently unbounded accumulator stays Mixed.
- [x] Check references, snapshots, overflow, exceptions, both IR modes and all targets.
- [ ] Measure the exact runtime-bound reproducer against pinned main, release and real PHP.
- [ ] Complete independent review, CI and the ready-for-review gates.

Potentially overflowing updates keep Mixed storage until a discarded scalar SSA
trial proves every update safe. Missing by-value instance-call metadata and
unchanged values passing through branch joins can hide otherwise bounded counters.

Calls keep their effects and dispatch. Array bounds and integer overflow remain
separate proofs. Dynamic receivers, unsupported method ABIs and unknown aliases
retain the conservative path.
