---
type: "Runbook"
title: "Symfony --web runbook: the rung ladder past the SIGSEGV, and the three shapes each new rung takes"
description: "RUNBOOK for examples/symfony app under web . The target is HTTP 404 with 39,315 bytes containing \"Welcome to Symfony!\" scratchpad/php.body is the reference body from real PHP . THE LOOP, one rung per cycle, ~3 minutes ea"
resource: "examples/symfony-app/public/index.php"
tags: ["session-learning", "symfony", "runbook", "web", "eval-bridge", "builtins"]
timestamp: "2026-09-12T20:06:27.233Z"
x-kage-id: "repo:lazy-petting-popcorn:runbook:symfony-web-runbook-the-rung-ladder-past-the-sigsegv-and-the-three-shapes-each-n"
x-kage-type: "runbook"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["examples/symfony-app/public/index.php", "crates/elephc-builtin-contract/src/catalog_data.rs", "src/name_resolver/mod.rs", "src/native_deps/toolchain.rs"]
x-kage-stack: ["php", "rust"]
---

# Symfony --web runbook: the rung ladder past the SIGSEGV, and the three shapes each new rung takes

> RUNBOOK for examples/symfony app under web . The target is HTTP 404 with 39,315 bytes containing "Welcome to Symfony!…

RUNBOOK for `examples/symfony-app` under `--web`. The target is HTTP 404 with 39,315 bytes containing "Welcome to Symfony!" (`scratchpad/php.body` is the reference body from real PHP).

THE LOOP, one rung per cycle, ~3 minutes each:
1. `scratchpad/build-symfony.sh` -- runs `elephc native install` in the SAME shell as the compile, because the native artifact cache key hashes PATH and TMPDIR (`src/native_deps/toolchain.rs:131,142`).
2. `scratchpad/serve-curl.sh <port>` -- the verdict line plus the worker's stderr, which is where the rung's error text is.
3. `scratchpad/trace-req.sh` -- same request under `ELEPHC_EVAL_TRACE=1` into `scratchpad/trace.log`. `grep -n RuntimeFatal trace.log | head -3` names the INNERMOST failing frame; the "Fatal error:" line at the end names the OUTERMOST one and is routinely misleading (it blamed HttpKernel.php:188 for a refusal inside `ErrorController::__invoke`).
4. Reduce into `scratchpad/redNN/` with `main.php` + `gen.php`, driven by `PROBE_INC=gen.php ./main`. `main.php` only does `include getenv('PROBE_INC')`, so the interpreted half can be edited and re-run WITHOUT recompiling -- that is what makes a reduction cycle seconds instead of minutes. Put the expected `php -n` output in a comment at the top.

THE THREE SHAPES a rung takes, in the order they were actually hit:
- A MISSING BUILTIN (`get_cfg_var`, the `gc_*` family). Five edits: a contract in `crates/elephc-builtin-contract/src/catalog_data.rs` (strictly name-sorted), the count in `registry.rs`'s `catalog_is_valid_and_complete_for_all_contract_surfaces`, a compiler spec under `src/builtins/<area>/`, an eval builtin under `crates/elephc-magician/src/interpreter/builtins/<area>/` plus its two dispatch arms, and the name in `src/name_resolver/mod.rs`'s global-fallback list -- WITHOUT the last one the error reads `Call to undefined function symfony\component\...\get_cfg_var()` with the namespace still attached, which is the tell.
- A MISSING METADATA PATH (first-class callables, statics). Always a case a `match` or a lookup chain never covered.
- A PER-CONTEXT STATE DIVERGENCE. Class metadata is process-global, everything else is per eval context.

WHAT NOT TO TRUST: a fatal that names a `filter_var()` warning or the `FileResource::$resource` critical is NOISE -- both appear on every run, including runs that get further. Read past them to the LAST line before `worker ... exited`.
Evidence: Five rungs cleared in one session with this loop: SIGSEGV -> `unsupported DynamicCall` (reflection) -> `undefined function get_cfg_var` -> `undefined function gc_enabled` -> `Typed static property LinkStub::$composerRoots` -> `unsupported MethodCall` on a freed `$event`.
Verified by: Each rung's fix verified by its own scratchpad reducer against `php -n` 8.5.10 output before rebuilding Symfony.

## Verification

Five rungs cleared in one session with this loop: SIGSEGV -> `unsupported DynamicCall` (reflection) -> `undefined function get_cfg_var` -> `undefined function gc_enabled` -> `Typed static property LinkStub::$composerRoots` -> `unsupported MethodCall` on a freed `$event`.

# Citations

[1] explicit_capture (2026-09-12T20:06:27.233Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:runbook:symfony-web-runbook-the-rung-ladder-past-the-sigsegv-and-the-three-shapes-each-n","title":"Symfony --web runbook: the rung ladder past the SIGSEGV, and the three shapes each new rung takes","summary":"RUNBOOK for examples/symfony app under web . The target is HTTP 404 with 39,315 bytes containing \"Welcome to Symfony!\" scratchpad/php.body is the reference body from real PHP . THE LOOP, one rung per cycle, ~3 minutes ea","body":"RUNBOOK for `examples/symfony-app` under `--web`. The target is HTTP 404 with 39,315 bytes containing \"Welcome to Symfony!\" (`scratchpad/php.body` is the reference body from real PHP).\n\nTHE LOOP, one rung per cycle, ~3 minutes each:\n1. `scratchpad/build-symfony.sh` -- runs `elephc native install` in the SAME shell as the compile, because the native artifact cache key hashes PATH and TMPDIR (`src/native_deps/toolchain.rs:131,142`).\n2. `scratchpad/serve-curl.sh <port>` -- the verdict line plus the worker's stderr, which is where the rung's error text is.\n3. `scratchpad/trace-req.sh` -- same request under `ELEPHC_EVAL_TRACE=1` into `scratchpad/trace.log`. `grep -n RuntimeFatal trace.log | head -3` names the INNERMOST failing frame; the \"Fatal error:\" line at the end names the OUTERMOST one and is routinely misleading (it blamed HttpKernel.php:188 for a refusal inside `ErrorController::__invoke`).\n4. Reduce into `scratchpad/redNN/` with `main.php` + `gen.php`, driven by `PROBE_INC=gen.php ./main`. `main.php` only does `include getenv('PROBE_INC')`, so the interpreted half can be edited and re-run WITHOUT recompiling -- that is what makes a reduction cycle seconds instead of minutes. Put the expected `php -n` output in a comment at the top.\n\nTHE THREE SHAPES a rung takes, in the order they were actually hit:\n- A MISSING BUILTIN (`get_cfg_var`, the `gc_*` family). Five edits: a contract in `crates/elephc-builtin-contract/src/catalog_data.rs` (strictly name-sorted), the count in `registry.rs`'s `catalog_is_valid_and_complete_for_all_contract_surfaces`, a compiler spec under `src/builtins/<area>/`, an eval builtin under `crates/elephc-magician/src/interpreter/builtins/<area>/` plus its two dispatch arms, and the name in `src/name_resolver/mod.rs`'s global-fallback list -- WITHOUT the last one the error reads `Call to undefined function symfony\\component\\...\\get_cfg_var()` with the namespace still attached, which is the tell.\n- A MISSING METADATA PATH (first-class callables, statics). Always a case a `match` or a lookup chain never covered.\n- A PER-CONTEXT STATE DIVERGENCE. Class metadata is process-global, everything else is per eval context.\n\nWHAT NOT TO TRUST: a fatal that names a `filter_var()` warning or the `FileResource::$resource` critical is NOISE -- both appear on every run, including runs that get further. Read past them to the LAST line before `worker ... exited`.\nEvidence: Five rungs cleared in one session with this loop: SIGSEGV -> `unsupported DynamicCall` (reflection) -> `undefined function get_cfg_var` -> `undefined function gc_enabled` -> `Typed static property LinkStub::$composerRoots` -> `unsupported MethodCall` on a freed `$event`.\nVerified by: Each rung's fix verified by its own scratchpad reducer against `php -n` 8.5.10 output before rebuilding Symfony.","type":"runbook","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony","runbook","web","eval-bridge","builtins"],"paths":["examples/symfony-app/public/index.php","crates/elephc-builtin-contract/src/catalog_data.rs","src/name_resolver/mod.rs","src/native_deps/toolchain.rs"],"stack":["php","rust"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T20:06:27.233Z"}],"context":{"fact":"RUNBOOK for `examples/symfony-app` under `--web`. The target is HTTP 404 with 39,315 bytes containing \"Welcome to Symfony!\" (`scratchpad/php.body` is the reference body from real PHP).","verification":"Five rungs cleared in one session with this loop: SIGSEGV -> `unsupported DynamicCall` (reflection) -> `undefined function get_cfg_var` -> `undefined function gc_enabled` -> `Typed static property LinkStub::$composerRoots` -> `unsupported MethodCall` on a freed `$event`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T20:06:27.233Z","path_fingerprints":[{"path":"examples/symfony-app/public/index.php","sha256":"c0696c30e0f1a223481e55a8078c5ebc600cf301b2cc9426c12f52ff8ffb861d","size":206},{"path":"crates/elephc-builtin-contract/src/catalog_data.rs","sha256":"ae064e4b892c8b500176fc7cae7cac91be8fd237baacabee3c5366300e2cdad0","size":522156},{"path":"src/name_resolver/mod.rs","sha256":"db335b1f126b82f8afc06fdbf75b067f333c016fe9fdaec8f589ff805ea43d85","size":7909},{"path":"src/native_deps/toolchain.rs","sha256":"ce3bb5ec19f0b2b939734a559a3b1546e76b10d429ae800f04ecf356cc035564","size":18571}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":180000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":705},"created_at":"2026-09-12T20:06:27.233Z","updated_at":"2026-09-12T20:06:27.233Z","author_branch":"reconcile/dirname-symfony"}
```

