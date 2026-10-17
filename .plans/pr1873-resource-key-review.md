# Resource keys in saved eval references

A new review on PR1568 (PRRT_kwDORs6r_s6thRaf) identified a regression inherited
from PR1873: native array lookup accepted a resource offset, but persistent-key
normalization returned None and the call argument path aborted eval.

- [x] Preserve the failing value-call, nested-call, by-reference mutation and closed-stream alias controls.
- [x] Match four reference outputs on PHP8.5.10 and pinned PHP8.2.34.
- [x] Normalize EVAL_TAG_RESOURCE through the existing integer-cast helper.
- [x] Preserve all previous native reference fixtures and add one four-case regression.
- [x] Pass95 reference unit tests and all three native regression tests in each IR mode.

The resource key becomes a Rust integer ID. Its temporary cast result is retired
by eval_int_value; the saved key remains independent of the stream and native cell.
The fixtures call error_reporting(0) to isolate this contract. Ordinary resource
lookup currently omits PHP's offset warning on main; that separate baseline is
recorded with the unsuppressed controls. All four regression fixtures fail before
the coercion fix even with diagnostics disabled.

Publish on the existing PR1873 fork branch, then publish the same helper and test
on the existing PR1568 branch. Keep one commit per branch and refresh main before
each push. The original seven max-key fixtures remain unchanged.

PR1568 follow-up also passes133 focused interpreter units, both Linux checks,
and80 native fixture executions across PHP82/85 and both IR modes. PR1873 is
updated atd50516bf8c. The next PR1568 push uses leasea38b488298.
