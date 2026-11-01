---
type: "Gotcha"
title: "__rt_sorted_name_search must be unconditional in the runtime (J4 shell methods reference it in every binary)"
description: "The J4 flat member tables made rt sorted name search a symbol referenced by EVERY compiled binary the always emitted ReflectionClass shell getMethod/getProperty bodies lower through the dynamic ReflectionMethod/Reflectio"
resource: "tests/codegen/support/runner.rs"
tags: ["session-learning"]
timestamp: "2026-09-02T08:50:34.252Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:rt-sorted-name-search-must-be-unconditional-in-the-runtime-j4-shell-methods-refe"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-verified: "verified"
x-kage-paths: ["tests/codegen/support/runner.rs"]
---

# __rt_sorted_name_search must be unconditional in the runtime (J4 shell methods reference it in every binary)

> The J4 flat member tables made rt sorted name search a symbol referenced by EVERY compiled binary the always emitted …

The J4 flat member tables made `__rt_sorted_name_search` a symbol referenced by EVERY compiled binary (the always-emitted ReflectionClass shell getMethod/getProperty bodies lower through the dynamic ReflectionMethod/ReflectionProperty dispatchers), so it must be emitted UNCONDITIONALLY in the runtime, not gated behind RuntimeFeatures introspection flags — gating it makes any test that links the shared `RuntimeFeatures::none()` runtime object (get_runtime_obj() in tests/codegen/support/runner.rs, used by tests/codegen/optimizer fixtures) fail at link time with "symbol not found". K1 fixed this in emit_runtime (src/codegen/runtime/emitters.rs). Also note: a `cargo test codegen_tests oop` filter also matches optimizer::constant_propagation::l**oop**s tests by substring.
Evidence: 16 tests in tests/codegen/optimizer/constant_propagation/loops failed at HEAD 2ea30720e with "__rt_sorted_name_search not found for architecture arm64"; reproduced with K1 changes reverted; all pass after ungating the emitter.
Verified by: cargo test --test codegen_tests constant_propagation (34 passed) after the fix; failing repro before.

## Verification

16 tests in tests/codegen/optimizer/constant_propagation/loops failed at HEAD 2ea30720e with "__rt_sorted_name_search not found for architecture arm64"; reproduced with K1 changes reverted; all pass after ungating the emitter.

# Citations

[1] explicit_capture (2026-07-19T14:57:43.807Z)
[2] reverification

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:rt-sorted-name-search-must-be-unconditional-in-the-runtime-j4-shell-methods-refe","title":"__rt_sorted_name_search must be unconditional in the runtime (J4 shell methods reference it in every binary)","summary":"The J4 flat member tables made rt sorted name search a symbol referenced by EVERY compiled binary the always emitted ReflectionClass shell getMethod/getProperty bodies lower through the dynamic ReflectionMethod/Reflectio","body":"The J4 flat member tables made `__rt_sorted_name_search` a symbol referenced by EVERY compiled binary (the always-emitted ReflectionClass shell getMethod/getProperty bodies lower through the dynamic ReflectionMethod/ReflectionProperty dispatchers), so it must be emitted UNCONDITIONALLY in the runtime, not gated behind RuntimeFeatures introspection flags — gating it makes any test that links the shared `RuntimeFeatures::none()` runtime object (get_runtime_obj() in tests/codegen/support/runner.rs, used by tests/codegen/optimizer fixtures) fail at link time with \"symbol not found\". K1 fixed this in emit_runtime (src/codegen/runtime/emitters.rs). Also note: a `cargo test codegen_tests oop` filter also matches optimizer::constant_propagation::l**oop**s tests by substring.\nEvidence: 16 tests in tests/codegen/optimizer/constant_propagation/loops failed at HEAD 2ea30720e with \"__rt_sorted_name_search not found for architecture arm64\"; reproduced with K1 changes reverted; all pass after ungating the emitter.\nVerified by: cargo test --test codegen_tests constant_propagation (34 passed) after the fix; failing repro before.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning"],"paths":["tests/codegen/support/runner.rs"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-07-19T14:57:43.807Z"},{"kind":"reverification","at":"2026-09-02T08:50:34.252Z","verified_by":"Symfony --web link on 8da22fe141, scratchpad/sf_build.log, 2026-09-02 09:14","evidence":"Cited tests/codegen/support/runner.rs changed only through checkpoint 52cfeb84d5 (in-flight test-support edits) and the in-progress merge of origin/main a7bb4cd92c. The claim itself was re-exercised on 2026-09-02: the full Symfony --web link on compiler 8da22fe141 (784 MB of emitted assembly, sf_build.log) reported exactly five undefined symbols, all profiler-bridge (_elephc_instr_*/_elephc_probe_*), and none for __rt_sorted_name_search, i.e. the runtime still emits it unconditionally.","changed_paths":[{"path":"tests/codegen/support/runner.rs","prior_sha256":"b007f641b247f955d5314f6275eb7dd1e0841b0a5224578505ab113683d67ee1","sha256":"1dbccdbab97165ff2db0606faa760a131bfba354896f0a2bcc15b1c2163c43ed"}]}],"context":{"fact":"The J4 flat member tables made `__rt_sorted_name_search` a symbol referenced by EVERY compiled binary (the always-emitted ReflectionClass shell getMethod/getProperty bodies lower through the dynamic ReflectionMethod/ReflectionProperty dispatchers), so it must be emitted UNCONDITIONALLY in the runtime, not gated behind RuntimeFeatures introspection flags — gating it makes any test that links the shared `RuntimeFeatures::none()` runtime object (get_runtime_obj() in tests/codegen/support/runner.rs, used by tests/codegen/optimizer fixtures) fail at link time with \"symbol not found\". K1 fixed this in emit_runtime (src/codegen/runtime/emitters.rs). Also note: a `cargo test codegen_tests oop` filter also matches optimizer::constant_propagation::l**oop**s tests by substring.\nEvidence: 16 tests in tests/codegen/optimizer/constant_propagation/loops failed at HEAD 2ea30720e with \"__rt_sorted_name_search not found for architecture arm64\"; reproduced with K1 changes reverted; all pass after ungating the emitter.\nVerified by: cargo test --test codegen_tests constant_propagation (34 passed) after the fix; failing repro before.","verification":"16 tests in tests/codegen/optimizer/constant_propagation/loops failed at HEAD 2ea30720e with \"__rt_sorted_name_search not found for architecture arm64\"; reproduced with K1 changes reverted; all pass after ungating the emitter."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-02T08:50:34.252Z","path_fingerprints":[{"path":"tests/codegen/support/runner.rs","sha256":"1dbccdbab97165ff2db0606faa760a131bfba354896f0a2bcc15b1c2163c43ed","size":39616}],"path_fingerprint_policy":"source_hash_staleness","verification":"evidence_reverification"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":100,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","concise but substantive","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":283,"reverified_at":"2026-09-02T08:50:34.252Z"},"created_at":"2026-07-19T14:57:43.807Z","updated_at":"2026-09-02T08:50:34.252Z","author_branch":"reconcile/dirname-symfony"}
```

