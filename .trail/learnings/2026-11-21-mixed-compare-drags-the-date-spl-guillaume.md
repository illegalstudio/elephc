---
title: Mixed compare drags the date/SPL runtime into feature-less programs
author: Guillaume
date: 2026-11-21
branch: bead-optimizer
touches:
  - src/ir_lower/function/object_comparisons.rs
  - src/ir_lower/builtin_datetime.rs
---

A relational compare on a mixed operand (e.g. a loop counter the checker leaves boxed) pulls in __elephc_internal_order_compare. Its DateTimeInterface branch calls getTimestamp()/format() on untyped operands; lowering that demanded every date, interval and __call class body (115k asm lines) and a __rt_member_exists reference that tests/codegen's feature-less runtime object cannot link ('linker failed' in optimizer tests). Fixed by gating the branch on builtin_datetime::module_may_hold_datetimes. The harness (tests/codegen/support/compiler.rs) differs from CLI --emit-asm: CLI output is not a reliable baseline for user_asm assertions; build the pre-merge commit and dump user_asm from the test instead.
