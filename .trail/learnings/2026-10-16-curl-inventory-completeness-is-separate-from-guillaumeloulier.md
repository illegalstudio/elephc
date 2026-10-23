---
title: cURL inventory completeness is separate from PHP runtime parity
author: Guillaumeloulier
date: 2026-10-16
branch: feat/curl-php-src-compliance
cost: { minutes: 1103, tokens: 8964260 }
touches:
  - docs/php/curl.md
  - src/curl_prelude/build.rs
  - src/curl_prelude/oracle.rs
  - scripts/curl/php_src_tests/run_php_src_tests.py
---

## Decided
At worktree HEAD c65ff1f9ec (audit on 2026-10-16), the frozen cURL inventory matches PHP-8.5 php-src b94db20dfe9b2e78984069d52af530563e647b44 by name: 35 functions, 6 classes and 689 constants. The upstream top-level ext/curl/tests directory contains 178 PHPTs. Inventory completeness is separate from runtime parity.
## Tried and ruled out
The documented 155 pass / 17 skip / 6 fail result was read from docs/php/curl.md, not reproduced. This worktree had no target artifacts or retained result JSON in its audit directory. Its runner evaluates SKIPIF using host PHP and normalizes diagnostic newlines; its JSON output does not record oracle/compiler hashes. Do not call this a fresh full-parity result or a five-target validation.
## Watch out
docs/php/curl.md is stale in several places: CURLINFO_HEADER_OUT is implemented by the prelude despite the rejection paragraph; curl_version already declares array|false; multi by-reference outputs are untyped. The AST builder is the production implementation and oracle.rs is its test oracle. CURLOPT_SSH_HOSTKEYFUNCTION validates then returns true without retaining/invoking the callback (build.rs decl_fn_curl_setopt); four stream options remain unavailable in eval. Blob options, FNMATCH_FUNCTION and multi PUSHFUNCTION remain gaps. Re-audit inherited diagnostics edits and runtime lifetime claims before relying on historical PHPT counts. Kioku had no worktree store; trail fell back to the root checkout graph, so branch-specific code was checked at the exact worktree paths.
