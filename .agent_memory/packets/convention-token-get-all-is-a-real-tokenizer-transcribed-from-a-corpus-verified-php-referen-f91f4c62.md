---
type: "Convention"
title: "token_get_all is a real tokenizer, transcribed from a corpus-verified PHP reference"
description: "src/tokenizer prelude.rs now declares a COMPLETE token get all plus token name through the synthetic class AST builders. It replaces the earlier empty array probe. How it was built, and how to extend it safely: 1. The im"
resource: "src/tokenizer_prelude.rs"
tags: ["session-learning", "tokenizer", "prelude", "symfony", "differential-testing"]
timestamp: "2026-09-13T08:51:18.194Z"
x-kage-id: "repo:lazy-petting-popcorn:convention:token-get-all-is-a-real-tokenizer-transcribed-from-a-corpus-verified-php-referen"
x-kage-type: "convention"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/tokenizer_prelude.rs", "src/types/token_constants.rs", "crates/elephc-magician/src/eval_php_profile.rs", "src/backend_gap_prelude.rs"]
---

# token_get_all is a real tokenizer, transcribed from a corpus-verified PHP reference

> src/tokenizer prelude.rs now declares a COMPLETE token get all plus token name through the synthetic class AST builde…

`src/tokenizer_prelude.rs` now declares a COMPLETE `token_get_all()` plus `token_name()` through the `synthetic_class` AST builders. It replaces the earlier empty-array probe.

How it was built, and how to extend it safely:
1. The implementation was written first as PLAIN PHP in the scratchpad and differential-tested against `php -n` 8.5.10's own `token_get_all()` over 3602 files -- the whole `examples/symfony-app/vendor` tree (1429) plus every PHP file under `examples/` (2173) -- comparing id, text, line and array-vs-string shape for EVERY token. Only then was it transcribed to builders. Rebuild that harness before changing the scanner; it found every rule below.
2. Five rules the first draft got wrong, each caught by the corpus and each measured rather than reasoned about:
   * php's ST_LOOKING_FOR_PROPERTY: the label after `->`/`?->` is T_STRING whatever it spells, and ONLY whitespace and comments may sit between -- a `$o->{'x'}` left the state armed and turned a later `throw` into a T_STRING.
   * `enum` is semi-reserved: T_ENUM only when a name follows, so `function enum()` and `enum(1)` are T_STRING.
   * `&` splits into T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG / the NOT_ variant by looking past whitespace AND comments for a `$` or a `...`.
   * `public(set)` / `private(set)` / `protected(set)` are ONE token whose text is the whole source span.
   * A `"` body must clear `hd_nowdoc`; a leftover from an earlier nowdoc suppressed interpolation entirely.
   * `|>` (php 8.5's pipe) is T_PIPE.
3. `src/types/token_constants.rs` now carries all 153 tokenizer constants (152 distinct values; T_PAAMAYIM_NEKUDOTAYIM aliases T_DOUBLE_COLON). The PHP 8.5 column is measured; every other column is that value plus the profile shift `[-4, -5, -5, 0, -1, 0, +6]`. That rule was DERIVED, not assumed: the twelve entries the table already carried are each exactly the 8.5 value plus the same per-profile shift, and a test now pins both the rule and the measured 8.5 values. `crates/elephc-magician/src/eval_php_profile.rs` carries the same table and must stay in sync.

The injection gate is `crate::backend_gap_prelude::inject_if_used`, on `usage.references("token_get_all") || usage.references("token_name")`.
Evidence: 3602-file differential against php -n 8.5.10's native token_get_all with zero mismatches, plus a hand-written edge-case file covering heredoc/nowdoc, every interpolation form, casts, attributes, asymmetric visibility, namespaced names and inline-html transitions.
Verified by: scratchpad/tok/diff.php over examples/symfony-app/vendor and examples/

## Verification

3602-file differential against php -n 8.5.10's native token_get_all with zero mismatches, plus a hand-written edge-case file covering heredoc/nowdoc, every interpolation form, casts, attributes, asymmetric visibility, namespaced names and inline-html transitions.

# Citations

[1] explicit_capture (2026-09-13T08:51:18.194Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:convention:token-get-all-is-a-real-tokenizer-transcribed-from-a-corpus-verified-php-referen","title":"token_get_all is a real tokenizer, transcribed from a corpus-verified PHP reference","summary":"src/tokenizer prelude.rs now declares a COMPLETE token get all plus token name through the synthetic class AST builders. It replaces the earlier empty array probe. How it was built, and how to extend it safely: 1. The im","body":"`src/tokenizer_prelude.rs` now declares a COMPLETE `token_get_all()` plus `token_name()` through the `synthetic_class` AST builders. It replaces the earlier empty-array probe.\n\nHow it was built, and how to extend it safely:\n1. The implementation was written first as PLAIN PHP in the scratchpad and differential-tested against `php -n` 8.5.10's own `token_get_all()` over 3602 files -- the whole `examples/symfony-app/vendor` tree (1429) plus every PHP file under `examples/` (2173) -- comparing id, text, line and array-vs-string shape for EVERY token. Only then was it transcribed to builders. Rebuild that harness before changing the scanner; it found every rule below.\n2. Five rules the first draft got wrong, each caught by the corpus and each measured rather than reasoned about:\n   * php's ST_LOOKING_FOR_PROPERTY: the label after `->`/`?->` is T_STRING whatever it spells, and ONLY whitespace and comments may sit between -- a `$o->{'x'}` left the state armed and turned a later `throw` into a T_STRING.\n   * `enum` is semi-reserved: T_ENUM only when a name follows, so `function enum()` and `enum(1)` are T_STRING.\n   * `&` splits into T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG / the NOT_ variant by looking past whitespace AND comments for a `$` or a `...`.\n   * `public(set)` / `private(set)` / `protected(set)` are ONE token whose text is the whole source span.\n   * A `\"` body must clear `hd_nowdoc`; a leftover from an earlier nowdoc suppressed interpolation entirely.\n   * `|>` (php 8.5's pipe) is T_PIPE.\n3. `src/types/token_constants.rs` now carries all 153 tokenizer constants (152 distinct values; T_PAAMAYIM_NEKUDOTAYIM aliases T_DOUBLE_COLON). The PHP 8.5 column is measured; every other column is that value plus the profile shift `[-4, -5, -5, 0, -1, 0, +6]`. That rule was DERIVED, not assumed: the twelve entries the table already carried are each exactly the 8.5 value plus the same per-profile shift, and a test now pins both the rule and the measured 8.5 values. `crates/elephc-magician/src/eval_php_profile.rs` carries the same table and must stay in sync.\n\nThe injection gate is `crate::backend_gap_prelude::inject_if_used`, on `usage.references(\"token_get_all\") || usage.references(\"token_name\")`.\nEvidence: 3602-file differential against php -n 8.5.10's native token_get_all with zero mismatches, plus a hand-written edge-case file covering heredoc/nowdoc, every interpolation form, casts, attributes, asymmetric visibility, namespaced names and inline-html transitions.\nVerified by: scratchpad/tok/diff.php over examples/symfony-app/vendor and examples/","type":"convention","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","tokenizer","prelude","symfony","differential-testing"],"paths":["src/tokenizer_prelude.rs","src/types/token_constants.rs","crates/elephc-magician/src/eval_php_profile.rs","src/backend_gap_prelude.rs"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T08:51:18.194Z"}],"context":{"fact":"`src/tokenizer_prelude.rs` now declares a COMPLETE `token_get_all()` plus `token_name()` through the `synthetic_class` AST builders. It replaces the earlier empty-array probe.","verification":"3602-file differential against php -n 8.5.10's native token_get_all with zero mismatches, plus a hand-written edge-case file covering heredoc/nowdoc, every interpolation form, casts, attributes, asymmetric visibility, namespaced names and inline-html transitions."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T08:51:18.194Z","path_fingerprints":[{"path":"src/tokenizer_prelude.rs","sha256":"6a2a8feb8bbf8091ff67d210517e2d741cc217a674fec000e627c89aa6fcb076","size":80169},{"path":"src/types/token_constants.rs","sha256":"ce4f0f864a53a0b45d272c1455bf5ea70c663824ffba11c06d6bdd839d54983d","size":14961},{"path":"crates/elephc-magician/src/eval_php_profile.rs","sha256":"afd4c74f5575d453f7b151cefc8f31b27d44cadeee20b4f9cce801bb846f5c44","size":16898},{"path":"src/backend_gap_prelude.rs","sha256":"92ccfd64d05746910c60920fe8fc15eee5bef5658115e08942c9aca919cf1639","size":31898}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[{"relation":"supersedes","to":"repo:lazy-petting-popcorn:gotcha:token-get-all-is-currently-a-probe-stub-in-src-tokenizer-prelude-rs-and-must-be-","evidence":"The probe stub was replaced by a complete tokenizer verified against php -n over 3602 files.","created_at":"2026-09-13T08:51:32.567Z"}],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":2000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":646},"created_at":"2026-09-13T08:51:18.194Z","updated_at":"2026-09-13T08:51:32.567Z","author_branch":"reconcile/dirname-symfony"}
```

