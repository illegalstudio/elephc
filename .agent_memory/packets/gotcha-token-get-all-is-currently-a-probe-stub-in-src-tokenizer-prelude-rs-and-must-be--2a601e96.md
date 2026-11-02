---
type: "Gotcha"
title: "token_get_all is currently a PROBE STUB in src/tokenizer_prelude.rs and must be replaced"
description: "src/tokenizer prelude.rs currently declares token get all string $code, int $flags = 0 : array returning an EMPTY ARRAY. It is a deliberate probe, not an implementation, added to measure how far a Symfony request gets pa"
resource: "src/tokenizer_prelude.rs"
tags: ["session-learning", "tokenizer", "prelude", "symfony", "incomplete"]
timestamp: "2026-09-13T08:31:23.486Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:token-get-all-is-currently-a-probe-stub-in-src-tokenizer-prelude-rs-and-must-be-"
x-kage-type: "gotcha"
x-kage-status: "superseded"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "superseded"
x-kage-paths: ["src/tokenizer_prelude.rs", "src/backend_gap_prelude.rs", "src/types/token_constants.rs"]
---

# token_get_all is currently a PROBE STUB in src/tokenizer_prelude.rs and must be replaced

> src/tokenizer prelude.rs currently declares token get all string $code, int $flags = 0 : array returning an EMPTY ARR…

`src/tokenizer_prelude.rs` currently declares `token_get_all(string $code, int $flags = 0): array` returning an EMPTY ARRAY. It is a deliberate probe, not an implementation, added to measure how far a Symfony request gets past `AttributeFileLoader::__construct`'s `\function_exists('token_get_all')` guard. It must be replaced with a real tokenizer before this branch is considered done.

What a real implementation needs:
- A scanner that preserves whitespace, comments, inline HTML, exact text and 1-based line numbers, returning `[id, text, line]` triples (single-character tokens stay plain strings).
- The T_* constants. `src/types/token_constants.rs` carries only the 13 version-sensitive ones; php 8.5 defines 153 (152 distinct values -- T_PAAMAYIM_NEKUDOTAYIM == T_DOUBLE_COLON). Their numeric values shift between php versions because they come from the bison-generated parser, and the existing table is per-profile, so the remaining ~140 cannot simply be pinned at the 8.5 values without colliding with the 13 already there under other profiles. Resolve that before adding them.
- `AttributeFileLoader::findClass` is the only caller on the Symfony path and needs T_INLINE_HTML, T_NS_SEPARATOR, T_STRING, T_NAME_QUALIFIED, T_CLASS, T_DOUBLE_COLON, T_NEW, T_WHITESPACE, T_DOC_COMMENT, T_COMMENT, T_NAMESPACE. In this app `src/Controller/` holds only a `.gitignore`, so `findClass` is never actually reached -- only the constructor's `function_exists` guard is.

The injection hook is `crate::backend_gap_prelude::inject_if_used`, gated on `usage.references("token_get_all")`, alongside the natural-order prelude.
Evidence: With the stub in place the Symfony request advanced from `LogicException: The Tokenizer extension is required for the routing attribute loader.` to the route dumper actually writing var/cache/dev/url_matching_routes.php.
Verified by: examples/symfony-app --web request progression

## Verification

With the stub in place the Symfony request advanced from `LogicException: The Tokenizer extension is required for the routing attribute loader.` to the route dumper actually writing var/cache/dev/url_matching_routes.php.

# Citations

[1] explicit_capture (2026-09-13T08:31:23.486Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:token-get-all-is-currently-a-probe-stub-in-src-tokenizer-prelude-rs-and-must-be-","title":"token_get_all is currently a PROBE STUB in src/tokenizer_prelude.rs and must be replaced","summary":"src/tokenizer prelude.rs currently declares token get all string $code, int $flags = 0 : array returning an EMPTY ARRAY. It is a deliberate probe, not an implementation, added to measure how far a Symfony request gets pa","body":"`src/tokenizer_prelude.rs` currently declares `token_get_all(string $code, int $flags = 0): array` returning an EMPTY ARRAY. It is a deliberate probe, not an implementation, added to measure how far a Symfony request gets past `AttributeFileLoader::__construct`'s `\\function_exists('token_get_all')` guard. It must be replaced with a real tokenizer before this branch is considered done.\n\nWhat a real implementation needs:\n- A scanner that preserves whitespace, comments, inline HTML, exact text and 1-based line numbers, returning `[id, text, line]` triples (single-character tokens stay plain strings).\n- The T_* constants. `src/types/token_constants.rs` carries only the 13 version-sensitive ones; php 8.5 defines 153 (152 distinct values -- T_PAAMAYIM_NEKUDOTAYIM == T_DOUBLE_COLON). Their numeric values shift between php versions because they come from the bison-generated parser, and the existing table is per-profile, so the remaining ~140 cannot simply be pinned at the 8.5 values without colliding with the 13 already there under other profiles. Resolve that before adding them.\n- `AttributeFileLoader::findClass` is the only caller on the Symfony path and needs T_INLINE_HTML, T_NS_SEPARATOR, T_STRING, T_NAME_QUALIFIED, T_CLASS, T_DOUBLE_COLON, T_NEW, T_WHITESPACE, T_DOC_COMMENT, T_COMMENT, T_NAMESPACE. In this app `src/Controller/` holds only a `.gitignore`, so `findClass` is never actually reached -- only the constructor's `function_exists` guard is.\n\nThe injection hook is `crate::backend_gap_prelude::inject_if_used`, gated on `usage.references(\"token_get_all\")`, alongside the natural-order prelude.\nEvidence: With the stub in place the Symfony request advanced from `LogicException: The Tokenizer extension is required for the routing attribute loader.` to the route dumper actually writing var/cache/dev/url_matching_routes.php.\nVerified by: examples/symfony-app --web request progression","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"superseded","confidence":0.7,"tags":["session-learning","tokenizer","prelude","symfony","incomplete"],"paths":["src/tokenizer_prelude.rs","src/backend_gap_prelude.rs","src/types/token_constants.rs"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T08:31:23.486Z"}],"context":{"fact":"`src/tokenizer_prelude.rs` currently declares `token_get_all(string $code, int $flags = 0): array` returning an EMPTY ARRAY. It is a deliberate probe, not an implementation, added to measure how far a Symfony request gets past `AttributeFileLoader::__construct`'s `\\function_exists('token_get_all')` guard. It must be replaced with a real tokenizer before this branch is considered done.","verification":"With the stub in place the Symfony request advanced from `LogicException: The Tokenizer extension is required for the routing attribute loader.` to the route dumper actually writing var/cache/dev/url_matching_routes.php."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T08:31:23.486Z","path_fingerprints":[{"path":"src/tokenizer_prelude.rs","sha256":"aec2191f3631642dec0c40ce4c6992796d7f88929ac3b01cf33e5859fc4bb540","size":1035},{"path":"src/backend_gap_prelude.rs","sha256":"47092fb506186a17707ad8cffc7a447b119a9fd9b76c19fe05552f7d837561f2","size":31856},{"path":"src/types/token_constants.rs","sha256":"bdc55be353d9d9bcf8ed2e316f05b914ff0428f381b2ac3c19c11406d7b7e187","size":4547}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture","superseded_at":"2026-09-13T08:51:32.567Z","superseded_by":"repo:lazy-petting-popcorn:convention:token-get-all-is-a-real-tokenizer-transcribed-from-a-corpus-verified-php-referen","superseded_reason":"The probe stub was replaced by a complete tokenizer verified against php -n over 3602 files."},"edges":[{"relation":"superseded_by","to":"repo:lazy-petting-popcorn:convention:token-get-all-is-a-real-tokenizer-transcribed-from-a-corpus-verified-php-referen","evidence":"The probe stub was replaced by a complete tokenizer verified against php -n over 3602 files.","created_at":"2026-09-13T08:51:32.567Z"}],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":478,"superseded_by":"repo:lazy-petting-popcorn:convention:token-get-all-is-a-real-tokenizer-transcribed-from-a-corpus-verified-php-referen","superseded_reason":"The probe stub was replaced by a complete tokenizer verified against php -n over 3602 files."},"created_at":"2026-09-13T08:31:23.486Z","updated_at":"2026-09-13T08:51:32.567Z","author_branch":"reconcile/dirname-symfony"}
```

