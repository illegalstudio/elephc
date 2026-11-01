---
title: Adding missing php extension functions natively (standard_gap_prelude workflow)
author: Guillaume
date: 2026-10-26
branch: reconcile/dirname-symfony
touches:
  - src/standard_gap_prelude.rs
---

## Decided
- Missing ext/standard functions go in `src/standard_gap_prelude.rs`: a `FUNCTIONS` table of (php name, AST builder) injected by `backend_gap_prelude::inject_if_used` when the program names one and declares none. One entry per function, no per-function branch.
- Workflow: write the function in plain PHP from php-src's algorithm (reference checkout: ~/PhpstormProjects/oss/php-src-8.5.10-oracle, read-only, no build dependency), run a differential oracle that loads it renamed `x_*` into real php next to the native function, then transcribe with `ELEPHC_TRANSCRIBE_WHICH=file ELEPHC_TRANSCRIBE_IN=... ELEPHC_TRANSCRIBE_OUT=... ELEPHC_TRANSCRIBE_SPLIT=<agg> cargo test --lib dump_prelude_on_request` and paste. Never parse PHP at compile time.
- Diagnostics from a prelude: `__elephc_internal_warning("Warning: ...\n")`; the prefix picks the level (Warning E_WARNING, Notice E_NOTICE, Deprecated E_DEPRECATED). Do not use `#[\Deprecated]` for an internal function: that is E_USER_DEPRECATED.
- Real coverage inventory = a probe of LITERAL `function_exists('x')`/`class_exists`/`defined` per php symbol (scratchpad extlit.php); a runtime-string `function_exists($n)` only sees linked functions in a closed world, and the builtin registry json undercounts preludes.
## Tried and ruled out
- `gettimeofday()` inside such a prelude: the name resolver rewrites it to `DateTime::__elephc_gettimeofday` BEFORE this injection, so the checker sees the raw name and refuses it. Use `microtime()`.
- `ctype_*` inside preludes: ctype is not implemented (Call to undefined function at run time); use strspn-based byte classes.
## Watch out
- A call to a function nothing declares compiles with NO diagnostic and fails only when it runs (str_rot13, uniqid, ctype_xdigit were all silently missing).
- Runtime builtins default to `MayAliasArguments` result ownership: an owned/leased argument is then never released. Any builtin whose result is fresh bytes (digests, formatted strings) must be listed `Independent` in `RuntimeFnId::result_ownership` or it leaks one block per call inside a function (md5/sha1/hash with a 2nd argument did).
