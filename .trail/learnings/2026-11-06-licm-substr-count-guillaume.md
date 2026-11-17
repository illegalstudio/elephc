# LICM hoists loop-invariant `substr_count()`

- The EIR LICM whitelist now admits `RuntimeFnId::SubstrCount` when both operands
  are strings and the needle is a non-empty literal. The call is byte-oriented,
  total for those arguments, and reads no mbstring state, so it bypasses the
  mbstring loop-writer restriction.
- The real blocker was the shared by-reference escape analysis: a runtime call
  with the loaded haystack as an operand marked the local address-escaping, so
  `immutable_local_loads` never refined the load and LICM could not hoist the
  call. `substr_count` is now in `runtime_call_observes_scalar_by_value`, its
  arguments are consumed by value, and the single entry store can be proven
  immutable.
- Regression: `tests/codegen/optimizer/eir_licm.rs::test_licm_hoists_invariant_substr_count_call`
  asserts the call is in the preheader with `origin: licm`, absent from the
  loop body, and still returns `7000` for a 700-match haystack over 10
  iterations.
- Native benchmark `benchmark/cases/native/substr_count` (20k iterations, 700
  matches each, process timing, 3 runs) with the release compiler:
  elephc 0.99 ms, PHP 130.27 ms, PHP+JIT 127.96 ms, C -O2 133.77 ms.
  This case now beats both PHP+JIT and C by ~130x.
- Focused regressions: `eir_licm` 10 passed, `substr_count` codegen 9 passed,
  `by_ref` 445 passed, `optimizer::` rerun captured in `/tmp/opt_licm.log`.
