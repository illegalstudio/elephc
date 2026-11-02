---
type: "Gotcha"
title: "The fake runtime does not refcount, so three PHP behaviours cannot be pinned"
description: "FakeOps , the eval interpreter's test runtime, does not model reference counts. retain returns the same handle and does nothing, and final object identity for release reports EVERY object release as final. Any test whose"
resource: "crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs"
tags: ["session-learning", "test-harness", "refcount", "fakeops", "eval-interpreter"]
timestamp: "2026-09-06T06:02:05.752Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:the-fake-runtime-does-not-refcount-so-three-php-behaviours-cannot-be-pinned-1788"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs", "crates/elephc-magician/src/interpreter/tests/support/mod.rs"]
x-kage-stack: ["rust", "php", "elephc-magician"]
---

# The fake runtime does not refcount, so three PHP behaviours cannot be pinned

> FakeOps , the eval interpreter's test runtime, does not model reference counts. retain returns the same handle and do…

`FakeOps`, the eval interpreter's test runtime, does not model reference counts. `retain` returns the same handle and does nothing, and `final_object_identity_for_release` reports EVERY object release as final. Any test whose expected output depends on an object having more than one live reference is therefore measuring the harness, not PHP.

Three real behaviours are implemented but unpinnable here as a result: the `IteratorAggregate` early-release ordering (php destroys the aggregate temporary as soon as `getIterator()` returns), a generator method's receiver outliving the call that built it (php prints `78d;`, the harness shows `d;78`), and any guard against over-releasing a borrowed cell.

This cuts both ways and is the dangerous part: a test written against this harness can be GREEN while asserting the opposite of PHP, and a genuine refcount fix looks like a no-op. Teaching FakeOps to count -- new_object gives 1, retain increments, release decrements, final only at zero -- is the highest-value test-harness item. It is not free: code paths that release cells they never retained would drive the count negative, so each one has to be found and either fixed or pinned as a deliberate borrow.
Evidence: lifecycle_scalars.rs: `final_object_identity_for_release` returns `Some(identity)` for every object without consulting any count; `retain` is documented as "Returns the same fake handle because fake cells do not refcount". Probe on `foreach ((new GC())->m() as $v) { echo $v; }` with a `__destruct`: php -n 8.5.6 prints `78d;`, the harness prints `d;78`.
Verified by: Direct read of the trait implementation plus a throwaway probe test run in the harness.

## Verification

lifecycle_scalars.rs: `final_object_identity_for_release` returns `Some(identity)` for every object without consulting any count; `retain` is documented as "Returns the same fake handle because fake cells do not refcount". Probe on `foreach ((new GC())->m() as $v) { echo $v; }` with a `__destruct`: php -n 8.5.6 prints `78d;`, the harness prints `d;78`.

# Citations

[1] explicit_capture (2026-09-06T06:02:05.752Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:the-fake-runtime-does-not-refcount-so-three-php-behaviours-cannot-be-pinned-1788","title":"The fake runtime does not refcount, so three PHP behaviours cannot be pinned","summary":"FakeOps , the eval interpreter's test runtime, does not model reference counts. retain returns the same handle and does nothing, and final object identity for release reports EVERY object release as final. Any test whose","body":"`FakeOps`, the eval interpreter's test runtime, does not model reference counts. `retain` returns the same handle and does nothing, and `final_object_identity_for_release` reports EVERY object release as final. Any test whose expected output depends on an object having more than one live reference is therefore measuring the harness, not PHP.\n\nThree real behaviours are implemented but unpinnable here as a result: the `IteratorAggregate` early-release ordering (php destroys the aggregate temporary as soon as `getIterator()` returns), a generator method's receiver outliving the call that built it (php prints `78d;`, the harness shows `d;78`), and any guard against over-releasing a borrowed cell.\n\nThis cuts both ways and is the dangerous part: a test written against this harness can be GREEN while asserting the opposite of PHP, and a genuine refcount fix looks like a no-op. Teaching FakeOps to count -- new_object gives 1, retain increments, release decrements, final only at zero -- is the highest-value test-harness item. It is not free: code paths that release cells they never retained would drive the count negative, so each one has to be found and either fixed or pinned as a deliberate borrow.\nEvidence: lifecycle_scalars.rs: `final_object_identity_for_release` returns `Some(identity)` for every object without consulting any count; `retain` is documented as \"Returns the same fake handle because fake cells do not refcount\". Probe on `foreach ((new GC())->m() as $v) { echo $v; }` with a `__destruct`: php -n 8.5.6 prints `78d;`, the harness prints `d;78`.\nVerified by: Direct read of the trait implementation plus a throwaway probe test run in the harness.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","test-harness","refcount","fakeops","eval-interpreter"],"paths":["crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs","crates/elephc-magician/src/interpreter/tests/support/mod.rs"],"stack":["rust","php","elephc-magician"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T06:02:05.752Z"}],"context":{"fact":"`FakeOps`, the eval interpreter's test runtime, does not model reference counts. `retain` returns the same handle and does nothing, and `final_object_identity_for_release` reports EVERY object release as final. Any test whose expected output depends on an object having more than one live reference is therefore measuring the harness, not PHP.","verification":"lifecycle_scalars.rs: `final_object_identity_for_release` returns `Some(identity)` for every object without consulting any count; `retain` is documented as \"Returns the same fake handle because fake cells do not refcount\". Probe on `foreach ((new GC())->m() as $v) { echo $v; }` with a `__destruct`: php -n 8.5.6 prints `78d;`, the harness prints `d;78`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T06:02:05.752Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/tests/support/runtime_ops/lifecycle_scalars.rs","sha256":"7aff91cb3bec00ff78f3f43193c0d55872b1853bfa074e8050c41bc677d4c36b","size":4111},{"path":"crates/elephc-magician/src/interpreter/tests/support/mod.rs","sha256":"0a92db9d0be9170215c76e58ac91b1a3a1e719238738a06f2b9696078bfa1987","size":11347}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":419},"created_at":"2026-09-06T06:02:05.752Z","updated_at":"2026-09-06T06:02:05.752Z","author_branch":"reconcile/dirname-symfony"}
```

