---
type: "Convention"
title: "Interpreted code cannot reach a prelude-provided builtin; give it a magician implementation"
description: "A builtin elephc serves as a conditionally injected PHP prelude src/var export prelude.rs , src/strnatcmp prelude.rs , … is injected only when the COMPILED program itself names the function. Interpreted code can never re"
resource: "crates/elephc-magician/src/interpreter/builtins/core/var_export.rs"
tags: ["session-learning", "builtins", "prelude", "eval-bridge", "var_export", "symfony"]
timestamp: "2026-09-13T08:31:30.626Z"
x-kage-id: "repo:lazy-petting-popcorn:convention:interpreted-code-cannot-reach-a-prelude-provided-builtin-give-it-a-magician-impl"
x-kage-type: "convention"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/builtins/core/var_export.rs", "crates/elephc-builtin-contract/src/catalog_surfaces.rs", "crates/elephc-builtin-contract/src/support.rs", "src/var_export_prelude.rs"]
---

# Interpreted code cannot reach a prelude-provided builtin; give it a magician implementation

> A builtin elephc serves as a conditionally injected PHP prelude src/var export prelude.rs , src/strnatcmp prelude.rs …

A builtin elephc serves as a conditionally injected PHP prelude (`src/var_export_prelude.rs`, `src/strnatcmp_prelude.rs`, …) is injected only when the COMPILED program itself names the function. Interpreted code can never reach it, and the call arrives as `call to undefined function X()`. Symfony hits this: its routing dumper is interpreted and called `var_export`.

The established shape (the `levenshtein` precedent) is:
- a `BuiltinKind::PreludeProvided` contract in `crates/elephc-builtin-contract/src/catalog_surfaces.rs` -- NOT a `catalog_data.rs` `Function` contract, which would wrongly demand an AOT `builtin!` registry binding the prelude route does not have;
- a real interpreter implementation with an `eval_builtin!` binding and a home file;
- census updates in `registry.rs` (`contracts().len()`) and `support.rs` (`eval_registry`, `interpreter_adapter`, `aot_registry`, `aot_external`). Note the `support.rs` numbers were already stale on this branch before `var_export`; they were recomputed rather than incremented blindly.

`var_export` was landed this way in `crates/elephc-magician/src/interpreter/builtins/core/var_export.rs`, transcribed from the prelude (which had been measured against `php -n`). Its output is byte-identical to `php -n` 8.5.10 for ints, bools, null, strings, nested arrays and the full float layout; the one divergence, `-0.0` rendering as `0.0`, is an upstream gap -- the interpreter loses the sign of negative zero entirely (`var_dump(-0.0)` prints `float(0)`) and `1/0.0` returns NULL instead of throwing DivisionByZeroError.
Evidence: scratchpad red30/a.php diffs 27 var_export cases against `php -n` 8.5.10 with a single divergence (-0.0). Regression test: tests/codegen/eval.rs::test_eval_var_export_matches_php_for_scalars_arrays_and_floats.
Verified by: cargo test --release --test codegen_tests -- codegen::eval::test_eval_var_export_matches_php; cargo test -p elephc-builtin-contract --lib

## Verification

scratchpad red30/a.php diffs 27 var_export cases against `php -n` 8.5.10 with a single divergence (-0.0). Regression test: tests/codegen/eval.rs::test_eval_var_export_matches_php_for_scalars_arrays_and_floats.

# Citations

[1] explicit_capture (2026-09-13T08:31:30.626Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:convention:interpreted-code-cannot-reach-a-prelude-provided-builtin-give-it-a-magician-impl","title":"Interpreted code cannot reach a prelude-provided builtin; give it a magician implementation","summary":"A builtin elephc serves as a conditionally injected PHP prelude src/var export prelude.rs , src/strnatcmp prelude.rs , … is injected only when the COMPILED program itself names the function. Interpreted code can never re","body":"A builtin elephc serves as a conditionally injected PHP prelude (`src/var_export_prelude.rs`, `src/strnatcmp_prelude.rs`, …) is injected only when the COMPILED program itself names the function. Interpreted code can never reach it, and the call arrives as `call to undefined function X()`. Symfony hits this: its routing dumper is interpreted and called `var_export`.\n\nThe established shape (the `levenshtein` precedent) is:\n- a `BuiltinKind::PreludeProvided` contract in `crates/elephc-builtin-contract/src/catalog_surfaces.rs` -- NOT a `catalog_data.rs` `Function` contract, which would wrongly demand an AOT `builtin!` registry binding the prelude route does not have;\n- a real interpreter implementation with an `eval_builtin!` binding and a home file;\n- census updates in `registry.rs` (`contracts().len()`) and `support.rs` (`eval_registry`, `interpreter_adapter`, `aot_registry`, `aot_external`). Note the `support.rs` numbers were already stale on this branch before `var_export`; they were recomputed rather than incremented blindly.\n\n`var_export` was landed this way in `crates/elephc-magician/src/interpreter/builtins/core/var_export.rs`, transcribed from the prelude (which had been measured against `php -n`). Its output is byte-identical to `php -n` 8.5.10 for ints, bools, null, strings, nested arrays and the full float layout; the one divergence, `-0.0` rendering as `0.0`, is an upstream gap -- the interpreter loses the sign of negative zero entirely (`var_dump(-0.0)` prints `float(0)`) and `1/0.0` returns NULL instead of throwing DivisionByZeroError.\nEvidence: scratchpad red30/a.php diffs 27 var_export cases against `php -n` 8.5.10 with a single divergence (-0.0). Regression test: tests/codegen/eval.rs::test_eval_var_export_matches_php_for_scalars_arrays_and_floats.\nVerified by: cargo test --release --test codegen_tests -- codegen::eval::test_eval_var_export_matches_php; cargo test -p elephc-builtin-contract --lib","type":"convention","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","builtins","prelude","eval-bridge","var_export","symfony"],"paths":["crates/elephc-magician/src/interpreter/builtins/core/var_export.rs","crates/elephc-builtin-contract/src/catalog_surfaces.rs","crates/elephc-builtin-contract/src/support.rs","src/var_export_prelude.rs"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T08:31:30.626Z"}],"context":{"fact":"A builtin elephc serves as a conditionally injected PHP prelude (`src/var_export_prelude.rs`, `src/strnatcmp_prelude.rs`, …) is injected only when the COMPILED program itself names the function. Interpreted code can never reach it, and the call arrives as `call to undefined function X()`. Symfony hits this: its routing dumper is interpreted and called `var_export`.","verification":"scratchpad red30/a.php diffs 27 var_export cases against `php -n` 8.5.10 with a single divergence (-0.0). Regression test: tests/codegen/eval.rs::test_eval_var_export_matches_php_for_scalars_arrays_and_floats."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T08:31:30.626Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/builtins/core/var_export.rs","sha256":"f0870129dedb355b4b04115c87099da545a7ae81b75161a041910ed4ccd43366","size":12871},{"path":"crates/elephc-builtin-contract/src/catalog_surfaces.rs","sha256":"9bbbf245acf1601ab728c20c4d788b1c75ab747ae1e006d4bce6cef04db718ee","size":6852},{"path":"crates/elephc-builtin-contract/src/support.rs","sha256":"62106742771c37c994b6154cd4105e0d04f9b77d610d596e74e4dcc981761e7c","size":17673},{"path":"src/var_export_prelude.rs","sha256":"31ec63f22149577f156f1a045335e7237a698464b8897ca70aa9f6f46c015c2f","size":23880}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":2000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":486},"created_at":"2026-09-13T08:31:30.626Z","updated_at":"2026-09-13T08:31:30.626Z","author_branch":"reconcile/dirname-symfony"}
```

