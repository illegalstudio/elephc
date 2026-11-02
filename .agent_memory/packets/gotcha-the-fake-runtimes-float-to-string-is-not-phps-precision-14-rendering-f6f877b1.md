---
type: "Gotcha"
title: "The fake runtime's float to string is not PHP's precision-14 rendering"
description: "FakeOps renders a float with Rust's shortest round trip repr, so 0.1 + 0.2 comes out 0.30000000000000004 and 0.0 comes out 0 . PHP's precision 14 conversion gives 0.3 and 0 . Every eval interpreter test that echoes, dump"
resource: "crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs"
tags: ["session-learning", "test-harness", "float-formatting", "fakeops", "eval-interpreter"]
timestamp: "2026-09-06T06:02:20.027Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:the-fake-runtimes-float-to-string-is-not-phps-precision-14-rendering-17886745400"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs", "crates/elephc-magician/src/interpreter/expressions/evaluation.rs", "crates/elephc-magician/src/interpreter/tests/control_flow.rs"]
x-kage-stack: ["rust", "php", "elephc-magician"]
---

# The fake runtime's float to string is not PHP's precision-14 rendering

> FakeOps renders a float with Rust's shortest round trip repr, so 0.1 + 0.2 comes out 0.30000000000000004 and 0.0 come…

`FakeOps` renders a float with Rust's shortest round-trip repr, so `0.1 + 0.2` comes out `0.30000000000000004` and `-0.0` comes out `0`. PHP's precision-14 conversion gives `0.3` and `-0`. Every eval interpreter test that echoes, dumps or interpolates a float is measuring Rust's formatting rather than PHP's, which means such a test can be green while pinning a value php never prints.

Found while pinning the message of `\UnhandledMatchError`, whose float rule is exactly PHP's `(string)` conversion plus the `.0` that conversion drops: `(string)1.0` is `1` but the message says `1.0`, and `1.0E+20` already carries a dot so it stays as it is. The message renderer is right against the real runtime; only the harness disagrees, so the float rows had to be left out of that test rather than pinned.
Evidence: php -n 8.5.6 on an unmatched match: `Unhandled match case 0.3`, `Unhandled match case -0.0`, `Unhandled match case 1.0`. The harness produced `0.30000000000000004`, `0.0` and `1.0` for the same three subjects.
Verified by: Measured with php -n 8.5.6 and observed as an assertion diff in execute_program_match_error_names_the_subject_the_way_php_does.

## Verification

php -n 8.5.6 on an unmatched match: `Unhandled match case 0.3`, `Unhandled match case -0.0`, `Unhandled match case 1.0`. The harness produced `0.30000000000000004`, `0.0` and `1.0` for the same three subjects.

# Citations

[1] explicit_capture (2026-09-06T06:02:20.027Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:the-fake-runtimes-float-to-string-is-not-phps-precision-14-rendering-17886745400","title":"The fake runtime's float to string is not PHP's precision-14 rendering","summary":"FakeOps renders a float with Rust's shortest round trip repr, so 0.1 + 0.2 comes out 0.30000000000000004 and 0.0 comes out 0 . PHP's precision 14 conversion gives 0.3 and 0 . Every eval interpreter test that echoes, dump","body":"`FakeOps` renders a float with Rust's shortest round-trip repr, so `0.1 + 0.2` comes out `0.30000000000000004` and `-0.0` comes out `0`. PHP's precision-14 conversion gives `0.3` and `-0`. Every eval interpreter test that echoes, dumps or interpolates a float is measuring Rust's formatting rather than PHP's, which means such a test can be green while pinning a value php never prints.\n\nFound while pinning the message of `\\UnhandledMatchError`, whose float rule is exactly PHP's `(string)` conversion plus the `.0` that conversion drops: `(string)1.0` is `1` but the message says `1.0`, and `1.0E+20` already carries a dot so it stays as it is. The message renderer is right against the real runtime; only the harness disagrees, so the float rows had to be left out of that test rather than pinned.\nEvidence: php -n 8.5.6 on an unmatched match: `Unhandled match case 0.3`, `Unhandled match case -0.0`, `Unhandled match case 1.0`. The harness produced `0.30000000000000004`, `0.0` and `1.0` for the same three subjects.\nVerified by: Measured with php -n 8.5.6 and observed as an assertion diff in execute_program_match_error_names_the_subject_the_way_php_does.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","test-harness","float-formatting","fakeops","eval-interpreter"],"paths":["crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs","crates/elephc-magician/src/interpreter/expressions/evaluation.rs","crates/elephc-magician/src/interpreter/tests/control_flow.rs"],"stack":["rust","php","elephc-magician"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T06:02:20.027Z"}],"context":{"fact":"`FakeOps` renders a float with Rust's shortest round-trip repr, so `0.1 + 0.2` comes out `0.30000000000000004` and `-0.0` comes out `0`. PHP's precision-14 conversion gives `0.3` and `-0`. Every eval interpreter test that echoes, dumps or interpolates a float is measuring Rust's formatting rather than PHP's, which means such a test can be green while pinning a value php never prints.","verification":"php -n 8.5.6 on an unmatched match: `Unhandled match case 0.3`, `Unhandled match case -0.0`, `Unhandled match case 1.0`. The harness produced `0.30000000000000004`, `0.0` and `1.0` for the same three subjects."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T06:02:20.027Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs","sha256":"7aff91cb3bec00ff78f3f43193c0d55872b1853bfa074e8050c41bc677d4c36b","size":4111},{"path":"crates/elephc-magician/src/interpreter/expressions/evaluation.rs","sha256":"ce7274783c0c11bad96a360143062368170a9a7f5e59a3055e39740f9977d8d2","size":23411},{"path":"crates/elephc-magician/src/interpreter/tests/control_flow.rs","sha256":"112aa956d99281bd06e716a08f53376baf4f75ea3a4744cc9f0dafcd7f86f962","size":24187}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":100,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","concise but substantive","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":291},"created_at":"2026-09-06T06:02:20.027Z","updated_at":"2026-09-06T06:02:20.027Z","author_branch":"reconcile/dirname-symfony"}
```

