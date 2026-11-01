---
type: "Gotcha"
title: "OPEN: demand-injected preludes are decided BEFORE autoload expansion, and forcing the hash prelude in afterwards trips a `hash_copy` EIR refusal"
description: "OPEN, measured, and REVERTED so the Symfony build stays green. Two separate defects, found together. DEFECT 1 — ORDERING. hash prelude::inject if used runs at pipeline.rs ~line 355, which is AFTER static include resoluti"
resource: "src/pipeline.rs"
tags: ["session-learning", "prelude", "pipeline", "autoload", "hash", "eir", "open"]
timestamp: "2026-09-12T21:12:16.482Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:open-demand-injected-preludes-are-decided-before-autoload-expansion-and-forcing-"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/pipeline.rs", "src/hash_prelude.rs", "src/codegen/lower_inst/builtins/io/resource_handles.rs", "src/hash_prelude/detect.rs"]
x-kage-stack: ["rust", "php"]
---

# OPEN: demand-injected preludes are decided BEFORE autoload expansion, and forcing the hash prelude in afterwards trips a `hash_copy` EIR refusal

> OPEN, measured, and REVERTED so the Symfony build stays green. Two separate defects, found together. DEFECT 1 — ORDER…

OPEN, measured, and REVERTED so the Symfony build stays green. Two separate defects, found together.

DEFECT 1 — ORDERING. `hash_prelude::inject_if_used` runs at `pipeline.rs` ~line 355, which is AFTER static include resolution but BEFORE `autoload::run_collecting_included` (~line 402). A `hash_init()` written in an AUTOLOADED class is therefore invisible to the detector, the prelude is not injected, and the compiled call has no declaration to reach. Symfony's `GlobResource::computeHash()` is exactly that shape. The pipeline already has a phase for this — "compat-preludes", documented as running "only after autoload expansion has exposed the complete closed-world program" — so the hash prelude (and any other demand-injected surface with the same exposure: image, pdo, tz, version) belongs there or needs a second chance there.

DEFECT 2 — WHAT BLOCKS THAT MOVE. Injecting the hash prelude into a program that ALSO loads code dynamically refuses at EIR with
  `unsupported EIR backend feature: hash_copy stream argument PHP type Str`
`hash_copy`'s body binds `$raw = $context->__elephc_ctx;` — a `mixed` property — and passes it to `__elephc_hash_ctx_copy`, whose lowering (`load_stream_fd_to_result`, `src/codegen/lower_inst/builtins/io/resource_handles.rs:42`) accepts `Resource`, `Mixed` and `Union` and refuses everything else. Something in the dynamic-code-load path types that read as `Str` — the class's OTHER property, `algo`, is the only `Str` in it, which makes a property-slot mix-up the first thing to check.

THE REDUCER IS 25 LINES: scratchpad/red22/main.php. `h.php` is the same file with the dynamic `include` removed and it compiles cleanly; `g.php` is the same file with the NAMESPACE removed and it still refuses. So the trigger is the dynamic include alone, not the namespace, and it needs no autoloading to reproduce — just `hash_init()` plus `include $path`.

CONSEQUENCE WHILE OPEN: the streaming hash surface is unavailable to autoloaded code. The Symfony request gets past `hash_final` anyway, because the NULL-handle bridge now shares one context and the INTERPRETER's own `hash_init`/`hash_update`/`hash_final` answer the calls.
Evidence: With a post-autoload second injection added, `elephc --web examples/symfony-app/public/index.php` refused with `unsupported EIR backend feature: hash_copy stream argument PHP type Str (line 1000809, op runtime_call)`; scratchpad/red22/main.php reproduces it at line 1000003 in 25 lines, and red22/h.php (no dynamic include) compiles.
Verified by: scratchpad/red22 g.php / h.php ablation; the Symfony build returning to exit 0 after the second injection was reverted.

## Verification

With a post-autoload second injection added, `elephc --web examples/symfony-app/public/index.php` refused with `unsupported EIR backend feature: hash_copy stream argument PHP type Str (line 1000809, op runtime_call)`; scratchpad/red22/main.php reproduces it at line 1000003 in 25 lines, and red22/h.php (no dynamic include) compiles.

# Citations

[1] explicit_capture (2026-09-12T21:12:16.482Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:open-demand-injected-preludes-are-decided-before-autoload-expansion-and-forcing-","title":"OPEN: demand-injected preludes are decided BEFORE autoload expansion, and forcing the hash prelude in afterwards trips a `hash_copy` EIR refusal","summary":"OPEN, measured, and REVERTED so the Symfony build stays green. Two separate defects, found together. DEFECT 1 — ORDERING. hash prelude::inject if used runs at pipeline.rs ~line 355, which is AFTER static include resoluti","body":"OPEN, measured, and REVERTED so the Symfony build stays green. Two separate defects, found together.\n\nDEFECT 1 — ORDERING. `hash_prelude::inject_if_used` runs at `pipeline.rs` ~line 355, which is AFTER static include resolution but BEFORE `autoload::run_collecting_included` (~line 402). A `hash_init()` written in an AUTOLOADED class is therefore invisible to the detector, the prelude is not injected, and the compiled call has no declaration to reach. Symfony's `GlobResource::computeHash()` is exactly that shape. The pipeline already has a phase for this — \"compat-preludes\", documented as running \"only after autoload expansion has exposed the complete closed-world program\" — so the hash prelude (and any other demand-injected surface with the same exposure: image, pdo, tz, version) belongs there or needs a second chance there.\n\nDEFECT 2 — WHAT BLOCKS THAT MOVE. Injecting the hash prelude into a program that ALSO loads code dynamically refuses at EIR with\n  `unsupported EIR backend feature: hash_copy stream argument PHP type Str`\n`hash_copy`'s body binds `$raw = $context->__elephc_ctx;` — a `mixed` property — and passes it to `__elephc_hash_ctx_copy`, whose lowering (`load_stream_fd_to_result`, `src/codegen/lower_inst/builtins/io/resource_handles.rs:42`) accepts `Resource`, `Mixed` and `Union` and refuses everything else. Something in the dynamic-code-load path types that read as `Str` — the class's OTHER property, `algo`, is the only `Str` in it, which makes a property-slot mix-up the first thing to check.\n\nTHE REDUCER IS 25 LINES: scratchpad/red22/main.php. `h.php` is the same file with the dynamic `include` removed and it compiles cleanly; `g.php` is the same file with the NAMESPACE removed and it still refuses. So the trigger is the dynamic include alone, not the namespace, and it needs no autoloading to reproduce — just `hash_init()` plus `include $path`.\n\nCONSEQUENCE WHILE OPEN: the streaming hash surface is unavailable to autoloaded code. The Symfony request gets past `hash_final` anyway, because the NULL-handle bridge now shares one context and the INTERPRETER's own `hash_init`/`hash_update`/`hash_final` answer the calls.\nEvidence: With a post-autoload second injection added, `elephc --web examples/symfony-app/public/index.php` refused with `unsupported EIR backend feature: hash_copy stream argument PHP type Str (line 1000809, op runtime_call)`; scratchpad/red22/main.php reproduces it at line 1000003 in 25 lines, and red22/h.php (no dynamic include) compiles.\nVerified by: scratchpad/red22 g.php / h.php ablation; the Symfony build returning to exit 0 after the second injection was reverted.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","prelude","pipeline","autoload","hash","eir","open"],"paths":["src/pipeline.rs","src/hash_prelude.rs","src/codegen/lower_inst/builtins/io/resource_handles.rs","src/hash_prelude/detect.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T21:12:16.482Z"}],"context":{"fact":"OPEN, measured, and REVERTED so the Symfony build stays green. Two separate defects, found together.","verification":"With a post-autoload second injection added, `elephc --web examples/symfony-app/public/index.php` refused with `unsupported EIR backend feature: hash_copy stream argument PHP type Str (line 1000809, op runtime_call)`; scratchpad/red22/main.php reproduces it at line 1000003 in 25 lines, and red22/h.php (no dynamic include) compiles."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T21:12:16.482Z","path_fingerprints":[{"path":"src/pipeline.rs","sha256":"466fb30fa137e2353e69b7e5bfb35cc15ce21e620bd340cd9ab4e2f06020fc14","size":40369},{"path":"src/hash_prelude.rs","sha256":"adc70ce5404873443723f8a02f3d187e409ff7d7b1dd4c7d027e66579bc18cac","size":13986},{"path":"src/codegen/lower_inst/builtins/io/resource_handles.rs","sha256":"7abb63546535e69e154105a9557204f03ddd913dbcc63a38c257bf8a071e9a45","size":18477},{"path":"src/hash_prelude/detect.rs","sha256":"92266c9630ca4db259831467bae874dddfe5aa5dd590792c089b4952be76bb7a","size":29522}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":90000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":661},"created_at":"2026-09-12T21:12:16.482Z","updated_at":"2026-09-12T21:12:16.482Z","author_branch":"reconcile/dirname-symfony"}
```

