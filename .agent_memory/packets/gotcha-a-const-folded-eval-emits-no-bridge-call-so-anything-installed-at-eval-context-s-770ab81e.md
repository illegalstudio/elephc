---
type: "Gotcha"
title: "A const-folded eval() emits no bridge call, so anything installed at eval-context setup never runs"
description: "ensure eval context in src/codegen/lower inst/builtins/eval/context registration.rs is where per binary eval configuration is installed regex provider, PHP profile, OPcache/runtime cache config, and now the opcache.file"
resource: "src/codegen/lower_inst/builtins/eval/context_registration.rs"
tags: ["session-learning", "eval", "codegen", "const-folding", "eval-bridge", "test-fixtures", "opcache"]
timestamp: "2026-09-12T16:37:06.904Z"
x-kage-id: "repo:sparkling-jingling-flute:gotcha:a-const-folded-eval-emits-no-bridge-call-so-anything-installed-at-eval-context-s"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/codegen/lower_inst/builtins/eval/context_registration.rs", "tests/opcache_file_cache_tests.rs", "crates/elephc-magician/src/ffi/context.rs"]
x-kage-stack: ["rust", "php"]
---

# A const-folded eval() emits no bridge call, so anything installed at eval-context setup never runs

> ensure eval context in src/codegen/lower inst/builtins/eval/context registration.rs is where per binary eval configur…

`ensure_eval_context` in src/codegen/lower_inst/builtins/eval/context_registration.rs is where per-binary eval configuration is installed (regex provider, PHP profile, OPcache/runtime-cache config, and now the `opcache.file_cache` startup validation). It is emitted at an eval SITE and guarded so it runs once. THE TRAP: an `eval()` whose argument folds to a constant is resolved at compile time and emits NO bridge call at all, so none of that setup happens.

Observed concretely while verifying the `opcache.file_cache` fatal:

  eval('$x = 1;');                        // const-folded: no bridge, no setup, program runs clean
  eval('include __DIR__ . "/lib.php";');  // reaches the bridge: setup runs, fatal fires

The first probe printed BEFORE/AFTER and exited 0 with a deliberately broken `opcache.file_cache`, which looked like the validation was broken. It was not — the binary simply had no eval bridge. Pinned by tests/opcache_file_cache_tests.rs::a_const_folded_eval_never_reaches_the_validation.

CONSEQUENCE FOR ANY FUTURE WORK: "the program contains eval()" is NOT the condition for bridge-installed behaviour; "the program reaches the eval bridge at run time" is, and that is a strictly narrower set. When writing a test fixture that must exercise anything installed in `ensure_eval_context`, use a DYNAMIC eval argument (a runtime include is the reliable one) — a constant string silently produces a binary that never calls into elephc-magician.

Also verified: output written before the first bridge-reaching eval is already flushed when a fatal raised there lands, so elephc prints it where reference PHP (validating before the script runs) prints nothing. Message and exit status 254 are identical; only the position differs.
Evidence: Compiled two fixtures with --ini opcache.enable_cli=1 --ini opcache.file_cache=/no/such/dir. The const-folded eval ran to completion (BEFORE/AFTER, exit 0); the dynamic-include eval printed BEFORE then the accelerator fatal and exited 254.
Verified by: cargo test --test opcache_file_cache_tests (12 passed), plus direct compile-and-run of both fixtures

## Verification

Compiled two fixtures with --ini opcache.enable_cli=1 --ini opcache.file_cache=/no/such/dir. The const-folded eval ran to completion (BEFORE/AFTER, exit 0); the dynamic-include eval printed BEFORE then the accelerator fatal and exited 254.

# Citations

[1] explicit_capture (2026-09-12T16:37:06.904Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:gotcha:a-const-folded-eval-emits-no-bridge-call-so-anything-installed-at-eval-context-s","title":"A const-folded eval() emits no bridge call, so anything installed at eval-context setup never runs","summary":"ensure eval context in src/codegen/lower inst/builtins/eval/context registration.rs is where per binary eval configuration is installed regex provider, PHP profile, OPcache/runtime cache config, and now the opcache.file","body":"`ensure_eval_context` in src/codegen/lower_inst/builtins/eval/context_registration.rs is where per-binary eval configuration is installed (regex provider, PHP profile, OPcache/runtime-cache config, and now the `opcache.file_cache` startup validation). It is emitted at an eval SITE and guarded so it runs once. THE TRAP: an `eval()` whose argument folds to a constant is resolved at compile time and emits NO bridge call at all, so none of that setup happens.\n\nObserved concretely while verifying the `opcache.file_cache` fatal:\n\n  eval('$x = 1;');                        // const-folded: no bridge, no setup, program runs clean\n  eval('include __DIR__ . \"/lib.php\";');  // reaches the bridge: setup runs, fatal fires\n\nThe first probe printed BEFORE/AFTER and exited 0 with a deliberately broken `opcache.file_cache`, which looked like the validation was broken. It was not — the binary simply had no eval bridge. Pinned by tests/opcache_file_cache_tests.rs::a_const_folded_eval_never_reaches_the_validation.\n\nCONSEQUENCE FOR ANY FUTURE WORK: \"the program contains eval()\" is NOT the condition for bridge-installed behaviour; \"the program reaches the eval bridge at run time\" is, and that is a strictly narrower set. When writing a test fixture that must exercise anything installed in `ensure_eval_context`, use a DYNAMIC eval argument (a runtime include is the reliable one) — a constant string silently produces a binary that never calls into elephc-magician.\n\nAlso verified: output written before the first bridge-reaching eval is already flushed when a fatal raised there lands, so elephc prints it where reference PHP (validating before the script runs) prints nothing. Message and exit status 254 are identical; only the position differs.\nEvidence: Compiled two fixtures with --ini opcache.enable_cli=1 --ini opcache.file_cache=/no/such/dir. The const-folded eval ran to completion (BEFORE/AFTER, exit 0); the dynamic-include eval printed BEFORE then the accelerator fatal and exited 254.\nVerified by: cargo test --test opcache_file_cache_tests (12 passed), plus direct compile-and-run of both fixtures","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval","codegen","const-folding","eval-bridge","test-fixtures","opcache"],"paths":["src/codegen/lower_inst/builtins/eval/context_registration.rs","tests/opcache_file_cache_tests.rs","crates/elephc-magician/src/ffi/context.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T16:37:06.904Z"}],"context":{"fact":"`ensure_eval_context` in src/codegen/lower_inst/builtins/eval/context_registration.rs is where per-binary eval configuration is installed (regex provider, PHP profile, OPcache/runtime-cache config, and now the `opcache.file_cache` startup validation). It is emitted at an eval SITE and guarded so it runs once. THE TRAP: an `eval()` whose argument folds to a constant is resolved at compile time and emits NO bridge call at all, so none of that setup happens.","verification":"Compiled two fixtures with --ini opcache.enable_cli=1 --ini opcache.file_cache=/no/such/dir. The const-folded eval ran to completion (BEFORE/AFTER, exit 0); the dynamic-include eval printed BEFORE then the accelerator fatal and exited 254."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T16:37:06.904Z","path_fingerprints":[{"path":"src/codegen/lower_inst/builtins/eval/context_registration.rs","sha256":"da684db5b65a7a8cf3a4d503ac57492517b61e0d0c96feb3076624a165fadaa1","size":11272},{"path":"tests/opcache_file_cache_tests.rs","sha256":"02ef11ac4b1a3b3ef2d0698f819f48e4a89fc2ea559010819b8e96333051cc8e","size":13759},{"path":"crates/elephc-magician/src/ffi/context.rs","sha256":"4f7ae42c2ef71e89f60db2a0a5618b259feaf42b9bac105356c7ca695d6507f0","size":18104}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":528,"stale":true,"stale_reasons":["linked path changed since memory was verified: src/codegen/lower_inst/builtins/eval/context_registration.rs, crates/elephc-magician/src/ffi/context.rs"],"suggested_action":"update"},"created_at":"2026-09-12T16:37:06.904Z","updated_at":"2026-09-12T18:12:29.320Z","author_branch":"feat/opcache-runtime-cache"}
```

