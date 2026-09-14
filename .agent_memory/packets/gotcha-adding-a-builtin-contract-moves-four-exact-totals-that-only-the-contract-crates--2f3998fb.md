---
type: "Gotcha"
title: "Adding a builtin contract moves four exact totals that only the contract crate's own tests check"
description: "Adding a BuiltinContract to catalog data.rs moves four hard coded totals, and cargo test test builtin parity tests does NOT see any of them — it was green while all four were wrong. The assertions live in the contract cr"
resource: "crates/elephc-builtin-contract/src/support.rs"
tags: ["session-learning", "builtins", "contract", "ci", "parity-gate"]
timestamp: "2026-09-11T17:01:32.621Z"
x-kage-id: "repo:sparkling-jingling-flute:gotcha:adding-a-builtin-contract-moves-four-exact-totals-that-only-the-contract-crates-"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-builtin-contract/src/support.rs", "crates/elephc-builtin-contract/src/registry.rs", "crates/elephc-builtin-contract/src/catalog_data.rs", ".github/workflows/ci.yml"]
x-kage-stack: ["rust"]
---

# Adding a builtin contract moves four exact totals that only the contract crate's own tests check

> Adding a BuiltinContract to catalog data.rs moves four hard coded totals, and cargo test test builtin parity tests do…

Adding a `BuiltinContract` to `catalog_data.rs` moves four hard-coded totals, and `cargo test --test builtin_parity_tests` does NOT see any of them — it was green while all four were wrong. The assertions live in the contract crate's own unit tests:

- `crates/elephc-builtin-contract/src/registry.rs` — `contracts().len()`
- `crates/elephc-builtin-contract/src/support.rs` — `eval_internal`, `unsupported`, `aot_registry`

The command that catches it is `cargo test -p elephc-builtin-contract --features curl`. `--features curl` matters: the curl surface is feature-gated in the catalog, and the CI job "Curl feature contract coverage" runs this exact configuration. Without the flag some of these totals are computed against a 34-contract-smaller catalog.

The full gate is five steps, all of which should be run locally before pushing a contract change:
```
cargo test -p elephc-builtin-contract --features curl
cargo test -p elephc-magician --features curl
cargo test --features curl --test builtin_parity_tests
cargo test -p elephc --lib --features curl builtins::parity_tests
python3 scripts/audit_builtin_eir_boundary.py --enforce-target-architecture
```
Plus the docs gate, which regenerates and requires a byte-identical tree: adding a contract shifts the `sidebar.order` of every builtin sorted after it, so `docs/internals/builtins/**` and `scripts/docs/builtin_registry.json` MUST be regenerated and committed (`cargo build --example gen_builtins --features curl`, then `extract_builtins.py --render --force`, `gen_module_sections.py`, `gen_php_comparison.py`).

Each total carries an explanatory comment that has to be updated with the number, not just the digit — the comments are the only record of what the count is made of.
Evidence: CI job "Curl feature contract coverage" failed on PR 968 while every locally run suite was green. Reproduced with `cargo test -p elephc-builtin-contract --features curl`: 3 failures, each off by exactly 3 — the number of contracts added.
Verified by: All five gate steps green locally after the fix, plus the regenerated docs tree.

## Verification

CI job "Curl feature contract coverage" failed on PR 968 while every locally run suite was green. Reproduced with `cargo test -p elephc-builtin-contract --features curl`: 3 failures, each off by exactly 3 — the number of contracts added.

# Citations

[1] explicit_capture (2026-09-11T17:01:32.621Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:gotcha:adding-a-builtin-contract-moves-four-exact-totals-that-only-the-contract-crates-","title":"Adding a builtin contract moves four exact totals that only the contract crate's own tests check","summary":"Adding a BuiltinContract to catalog data.rs moves four hard coded totals, and cargo test test builtin parity tests does NOT see any of them — it was green while all four were wrong. The assertions live in the contract cr","body":"Adding a `BuiltinContract` to `catalog_data.rs` moves four hard-coded totals, and `cargo test --test builtin_parity_tests` does NOT see any of them — it was green while all four were wrong. The assertions live in the contract crate's own unit tests:\n\n- `crates/elephc-builtin-contract/src/registry.rs` — `contracts().len()`\n- `crates/elephc-builtin-contract/src/support.rs` — `eval_internal`, `unsupported`, `aot_registry`\n\nThe command that catches it is `cargo test -p elephc-builtin-contract --features curl`. `--features curl` matters: the curl surface is feature-gated in the catalog, and the CI job \"Curl feature contract coverage\" runs this exact configuration. Without the flag some of these totals are computed against a 34-contract-smaller catalog.\n\nThe full gate is five steps, all of which should be run locally before pushing a contract change:\n```\ncargo test -p elephc-builtin-contract --features curl\ncargo test -p elephc-magician --features curl\ncargo test --features curl --test builtin_parity_tests\ncargo test -p elephc --lib --features curl builtins::parity_tests\npython3 scripts/audit_builtin_eir_boundary.py --enforce-target-architecture\n```\nPlus the docs gate, which regenerates and requires a byte-identical tree: adding a contract shifts the `sidebar.order` of every builtin sorted after it, so `docs/internals/builtins/**` and `scripts/docs/builtin_registry.json` MUST be regenerated and committed (`cargo build --example gen_builtins --features curl`, then `extract_builtins.py --render --force`, `gen_module_sections.py`, `gen_php_comparison.py`).\n\nEach total carries an explanatory comment that has to be updated with the number, not just the digit — the comments are the only record of what the count is made of.\nEvidence: CI job \"Curl feature contract coverage\" failed on PR 968 while every locally run suite was green. Reproduced with `cargo test -p elephc-builtin-contract --features curl`: 3 failures, each off by exactly 3 — the number of contracts added.\nVerified by: All five gate steps green locally after the fix, plus the regenerated docs tree.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","builtins","contract","ci","parity-gate"],"paths":["crates/elephc-builtin-contract/src/support.rs","crates/elephc-builtin-contract/src/registry.rs","crates/elephc-builtin-contract/src/catalog_data.rs",".github/workflows/ci.yml"],"stack":["rust"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T17:01:32.621Z"}],"context":{"fact":"Adding a `BuiltinContract` to `catalog_data.rs` moves four hard-coded totals, and `cargo test --test builtin_parity_tests` does NOT see any of them — it was green while all four were wrong. The assertions live in the contract crate's own unit tests:","verification":"CI job \"Curl feature contract coverage\" failed on PR 968 while every locally run suite was green. Reproduced with `cargo test -p elephc-builtin-contract --features curl`: 3 failures, each off by exactly 3 — the number of contracts added."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T17:01:32.621Z","path_fingerprints":[{"path":"crates/elephc-builtin-contract/src/support.rs","sha256":"d0fa23f242ca675e7f77be6df2bb0a9f3a4eafcec0ce73be75350931b2ccf57a","size":20030},{"path":"crates/elephc-builtin-contract/src/registry.rs","sha256":"9bb971c6690642760f58f3fd4a4071c18220205dc5767400d90a5c9fb1d9f31c","size":7447},{"path":"crates/elephc-builtin-contract/src/catalog_data.rs","sha256":"07b8b5fed4330c52d596e50dea33e8f6b85d595546e15feb0f2c3d4c7a0ab64a","size":981407},{"path":".github/workflows/ci.yml","sha256":"d72c5eadf8c139fb09edbb2e7b7f959fc4a665c94c17e90ee9b35a11a35dc83f","size":82779}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":521,"stale":true,"stale_reasons":["linked path changed since memory was verified: crates/elephc-builtin-contract/src/support.rs, crates/elephc-builtin-contract/src/registry.rs, crates/elephc-builtin-contract/src/catalog_data.rs"],"suggested_action":"update"},"created_at":"2026-09-11T17:01:32.621Z","updated_at":"2026-09-12T18:12:29.322Z","author_branch":"feat/opcache-runtime-cache"}
```

