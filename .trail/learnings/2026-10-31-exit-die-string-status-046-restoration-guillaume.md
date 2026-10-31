---
title: exit/die string status + 046 restoration
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/types/checker/builtins/language_constructs.rs
  - src/codegen/lower_inst/builtins/system.rs
  - crates/elephc-builtin-contract/src/catalog_surfaces.rs
  - crates/elephc-magician/src/interpreter/builtins/core/exit.rs
  - tests/codegen/array_basics.rs
  - tests/error_tests/misc/system_builtins.rs
  - scripts/filter/port_phpt.py
---

exit/die now accept a static string status, write it through __rt_stdout_write, flush active output buffers via abi::emit_exit(0), and terminate with status zero. Int/bool pass through the existing dynamic exit path; float uses the shared PHP float-to-int conversion; null exits zero. The language-construct checker accepts the string|int contract and PHP's weak bool/float/null coercions, while arrays/objects keep a frontend diagnostic. The catalog surface for exit and die now advertises string|int, and the eval exit helper echoes a string status before exiting zero. 046.phpt is restored to its original die("failed: unknown value for PHP_MAX_INT") branch and passes. Remaining gap: Mixed/Union status expressions need runtime tag dispatch (elephc-vqu).
