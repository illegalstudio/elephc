---
type: "Bug Fix"
title: "The eval interpreter's prelude-name handlers shadow the program's own declaration of that name"
description: "crates/elephc magician/src/interpreter/expressions/calls.rs intercepts the prelude provided OPcache names opcache get status , opcache reset , opcache get configuration , and the five file functions BEFORE it reaches con"
resource: "crates/elephc-magician/src/interpreter/expressions/calls.rs"
tags: ["session-learning", "opcache", "eval-bridge", "dispatch", "consistency"]
timestamp: "2026-09-11T16:37:44.822Z"
x-kage-id: "repo:sparkling-jingling-flute:bug_fix:the-eval-interpreters-prelude-name-handlers-shadow-the-programs-own-declaration-"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/expressions/calls.rs", "crates/elephc-magician/src/interpreter/builtins/registry/dispatch/mod.rs", "tests/opcache_runtime_cache_tests.rs"]
x-kage-stack: ["rust", "php"]
---

# The eval interpreter's prelude-name handlers shadow the program's own declaration of that name

> crates/elephc magician/src/interpreter/expressions/calls.rs intercepts the prelude provided OPcache names opcache get…

`crates/elephc-magician/src/interpreter/expressions/calls.rs` intercepts the prelude-provided OPcache names (`opcache_get_status`, `opcache_reset`, `opcache_get_configuration`, and the five file functions) BEFORE it reaches `context.function(name)` / `context.native_function(name)`. Those handlers were written when they were terminal falses, so shadowing cost nothing.

CONSEQUENCE, measured on the branch point as well as on HEAD: `opcache_get_status()` written natively returned the full array while the SAME call written inside `eval()` returned `false`, in one binary, with the cache enabled. One binary, two answers.

FIX for `opcache_get_status`: guard the intercept with `context.native_function(name).is_none()`. The prelude injects a REAL PHP function, and the eval bridge already registers eligible AOT global functions with the eval context, so falling through calls the body that knows both the compile-time manifest and the live runtime cache. The two positions then agree BY CONSTRUCTION rather than by keeping two implementations in step. The handler remains correct for a program with no such declaration — above all the compile-time const-folder, where nothing has been injected.

It also fixes a second case nobody had noticed: a program declaring its OWN `opcache_get_status()`. The prelude already skips injection then, but the eval stub shadowed the user's function anyway.

🔴 STILL OPEN: the same shadowing applies to `opcache_reset`, `opcache_get_configuration`, `opcache_is_script_cached`, `opcache_invalidate`, `opcache_compile_file` and `opcache_is_script_cached_in_file_cache`. The file functions now read the live runtime cache from eval, so they agree about the DYNAMIC tier — but not about the compile-time MANIFEST, which only the native body knows. `opcache_jit_blacklist` takes a `Closure` and should be checked separately before the same guard is applied to it.
Evidence: A probe printing both positions in one binary compiled with `--ini opcache.enable_cli=1`: before, `native=array` / `inside=false`; after, both report `array` with identical `num_cached_scripts`, `misses`, `count($scripts)` and key order. Reproduced identically with the pre-branch compiler, which is what establishes it was not a regression.
Verified by: tests/opcache_runtime_cache_tests.rs::the_status_array_is_the_same_written_natively_and_inside_eval, plus the magician lib suite (1249) and four OPcache integration suites.

## Verification

A probe printing both positions in one binary compiled with `--ini opcache.enable_cli=1`: before, `native=array` / `inside=false`; after, both report `array` with identical `num_cached_scripts`, `misses`, `count($scripts)` and key order. Reproduced identically with the pre-branch compiler, which is what establishes it was not a regression.

# Citations

[1] explicit_capture (2026-09-11T16:37:44.822Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:bug_fix:the-eval-interpreters-prelude-name-handlers-shadow-the-programs-own-declaration-","title":"The eval interpreter's prelude-name handlers shadow the program's own declaration of that name","summary":"crates/elephc magician/src/interpreter/expressions/calls.rs intercepts the prelude provided OPcache names opcache get status , opcache reset , opcache get configuration , and the five file functions BEFORE it reaches con","body":"`crates/elephc-magician/src/interpreter/expressions/calls.rs` intercepts the prelude-provided OPcache names (`opcache_get_status`, `opcache_reset`, `opcache_get_configuration`, and the five file functions) BEFORE it reaches `context.function(name)` / `context.native_function(name)`. Those handlers were written when they were terminal falses, so shadowing cost nothing.\n\nCONSEQUENCE, measured on the branch point as well as on HEAD: `opcache_get_status()` written natively returned the full array while the SAME call written inside `eval()` returned `false`, in one binary, with the cache enabled. One binary, two answers.\n\nFIX for `opcache_get_status`: guard the intercept with `context.native_function(name).is_none()`. The prelude injects a REAL PHP function, and the eval bridge already registers eligible AOT global functions with the eval context, so falling through calls the body that knows both the compile-time manifest and the live runtime cache. The two positions then agree BY CONSTRUCTION rather than by keeping two implementations in step. The handler remains correct for a program with no such declaration — above all the compile-time const-folder, where nothing has been injected.\n\nIt also fixes a second case nobody had noticed: a program declaring its OWN `opcache_get_status()`. The prelude already skips injection then, but the eval stub shadowed the user's function anyway.\n\n🔴 STILL OPEN: the same shadowing applies to `opcache_reset`, `opcache_get_configuration`, `opcache_is_script_cached`, `opcache_invalidate`, `opcache_compile_file` and `opcache_is_script_cached_in_file_cache`. The file functions now read the live runtime cache from eval, so they agree about the DYNAMIC tier — but not about the compile-time MANIFEST, which only the native body knows. `opcache_jit_blacklist` takes a `Closure` and should be checked separately before the same guard is applied to it.\nEvidence: A probe printing both positions in one binary compiled with `--ini opcache.enable_cli=1`: before, `native=array` / `inside=false`; after, both report `array` with identical `num_cached_scripts`, `misses`, `count($scripts)` and key order. Reproduced identically with the pre-branch compiler, which is what establishes it was not a regression.\nVerified by: tests/opcache_runtime_cache_tests.rs::the_status_array_is_the_same_written_natively_and_inside_eval, plus the magician lib suite (1249) and four OPcache integration suites.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","opcache","eval-bridge","dispatch","consistency"],"paths":["crates/elephc-magician/src/interpreter/expressions/calls.rs","crates/elephc-magician/src/interpreter/builtins/registry/dispatch/mod.rs","tests/opcache_runtime_cache_tests.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T16:37:44.822Z"}],"context":{"fact":"`crates/elephc-magician/src/interpreter/expressions/calls.rs` intercepts the prelude-provided OPcache names (`opcache_get_status`, `opcache_reset`, `opcache_get_configuration`, and the five file functions) BEFORE it reaches `context.function(name)` / `context.native_function(name)`. Those handlers were written when they were terminal falses, so shadowing cost nothing.","verification":"A probe printing both positions in one binary compiled with `--ini opcache.enable_cli=1`: before, `native=array` / `inside=false`; after, both report `array` with identical `num_cached_scripts`, `misses`, `count($scripts)` and key order. Reproduced identically with the pre-branch compiler, which is what establishes it was not a regression."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T16:37:44.822Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/expressions/calls.rs","sha256":"c1a9b129f98b8ae52241da41356084794f1c21a1b87d42878da550f981571064","size":11311},{"path":"crates/elephc-magician/src/interpreter/builtins/registry/dispatch/mod.rs","sha256":"1bf591a5ade5fe3e548a4b48ad9de3e568cf425505d15a3f2d8cee29a3db20e0","size":4566},{"path":"tests/opcache_runtime_cache_tests.rs","sha256":"be8004efcae2488e089c831a8a9ee345c6b307ba12aa74dd3022036b76c5ad76","size":13335}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":609,"stale":true,"stale_reasons":["linked path changed since memory was verified: tests/opcache_runtime_cache_tests.rs"],"suggested_action":"update"},"created_at":"2026-09-11T16:37:44.822Z","updated_at":"2026-09-12T18:12:29.316Z","author_branch":"feat/opcache-runtime-cache"}
```

