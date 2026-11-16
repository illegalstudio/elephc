---
title: "GMP corpus: 128 MiB test heap, __FILE__ fixtures, owned packet copy"
author: Guillaume
date: 2026-10-27
branch: feat/gmp-extension
cost: { minutes: 30, tokens: 808710 }
touches:
  - tests/codegen/support/compiler.rs
  - src/builtins/string/__elephc_string_copy.rs
  - src/gmp_prelude/validators.rs
---

Three fixes that made the Rust PHPT gate green (97/97 runnable ext/gmp cases): (1) the in-process fixture pipeline never wrote dir/test.php, so gmp_div_q/qr/r and divexact failed when they fopen(__FILE__, 'r'); the test support now writes the source at the synthetic main path, matching the CLI. (2) The GMP corpus runs with 128 MiB to mirror PHP CLI's memory_limit; 002.phpt computes 1000! and the default 8 MiB transport heap exhausts. (3) Returning substr(packet,1) leaked the extern packet because the callee may alias its string argument; returning substr(...) . '' made it fresh but the concat path crashed deep recursive factorial calls. Added internal __elephc_string_copy (Op::StrPersist) so packet unwrapping is a direct owned heap copy with no concat scratch, and the alias summary is Fresh; this keeps bug67917 leak-free and 002 stable.
