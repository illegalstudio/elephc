---
type: "Bug Fix"
title: "Parent-class override compatibility matched php in 13 of 14 shapes; the gap was the by-ref return"
description: "Fourteen override shapes were measured against php n 8.5.6 and against the eval interpreter. THIRTEEN already agreed parameter contravariance, return covariance, adding vs dropping a return type, all three arity cases, n"
resource: "crates/elephc-magician/src/interpreter/statements/interface_member_validation.rs"
tags: ["session-learning", "eval-interpreter", "inheritance", "override-compatibility", "by-ref-return", "php-conformance"]
timestamp: "2026-09-06T13:12:46.797Z"
x-kage-id: "repo:lazy-petting-popcorn:bug_fix:parent-class-override-compatibility-matched-php-in-13-of-14-shapes-the-gap-was-t"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/statements/interface_member_validation.rs", "crates/elephc-magician/src/interpreter/statements/interface_contracts.rs", "crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs"]
x-kage-stack: ["rust", "php"]
---

# Parent-class override compatibility matched php in 13 of 14 shapes; the gap was the by-ref return

> Fourteen override shapes were measured against php n 8.5.6 and against the eval interpreter. THIRTEEN already agreed …

Fourteen override shapes were measured against `php -n` 8.5.6 and against the eval interpreter. THIRTEEN already agreed -- parameter contravariance, return covariance, adding vs dropping a return type, all three arity cases, narrowed visibility, changed staticness, overriding `final`, and ADDING a by-ref return. Only one diverged, and it is worth recording so nobody re-audits the other thirteen: a subclass could DROP the parent method's `&` and nothing said so. php refuses that with `Fatal error: Declaration of C::f() must be compatible with & P::f()` -- the `&` prefixes the PARENT signature in the message, which is how php names the side that wanted it.

The root cause is a duplication that never happened: the one-directional by-ref rule (an override may ADD `&`, never DROP one) was written in `class_method_satisfies_interface_signature_with_return_mode` (interpreter/statements/interface_contracts.rs) for the INTERFACE path, and `class_method_signature_accepts` (interpreter/statements/interface_member_validation.rs) -- the PARENT-CLASS path, one file away and structurally parallel -- simply had no such check. The fix factors it into `override_by_ref_return_is_compatible(override_by_ref, required_by_ref)` called from both, rather than writing it a second time.

php's exact refusal wordings for this family, all measured:
- `Declaration of C::f() must be compatible with & P::f()` (dropped by-ref)
- `Declaration of C3::f(int $a) must be compatible with P3::f(mixed $a)` (narrowed parameter)
- `Declaration of C5::f(): mixed must be compatible with P5::f(): int` (widened return)
- `Declaration of C8::f() must be compatible with P8::f(): int` (dropped return type)
- `Declaration of C9::f($a) must be compatible with P9::f($a, $b)` (fewer parameters)
- `Declaration of CB::f($a, $b) must be compatible with PB::f($a)` (extra REQUIRED parameter; an extra parameter WITH a default is accepted)
- `Access level to CC::f() must be public (as in class PC)` (narrowed visibility)
- `Cannot make non static method PD::f() static in class CD`
- `Cannot override final method PE::f()`

Method for this kind of audit: write a throwaway `#[ignore]` probe test that runs every shape through the interpreter and prints one line each, and a php script that runs the same shapes through `php -n -r` via shell_exec with the `in Command line code on line N` suffix stripped. Diff the two tables. That located a single one-line gap in a surface that would otherwise have looked like a large piece of work.
Evidence: Seven tests in crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs covering all fourteen shapes, each expectation measured with php -n 8.5.6. Sentinel: with the by-ref rule removed from the parent path, exactly the one test that measures it fails and the other thirteen shapes stay green.
Verified by: cargo test -p elephc-magician --lib &lt; /dev/null -- 1478 passed, 0 failed, 0 filtered. Commit 9fc72f502b.

## Verification

Seven tests in crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs covering all fourteen shapes, each expectation measured with php -n 8.5.6. Sentinel: with the by-ref rule removed from the parent path, exactly the one test that measures it fails and the other thirteen shapes stay green.

# Citations

[1] explicit_capture (2026-09-06T13:12:46.797Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:bug_fix:parent-class-override-compatibility-matched-php-in-13-of-14-shapes-the-gap-was-t","title":"Parent-class override compatibility matched php in 13 of 14 shapes; the gap was the by-ref return","summary":"Fourteen override shapes were measured against php n 8.5.6 and against the eval interpreter. THIRTEEN already agreed parameter contravariance, return covariance, adding vs dropping a return type, all three arity cases, n","body":"Fourteen override shapes were measured against `php -n` 8.5.6 and against the eval interpreter. THIRTEEN already agreed -- parameter contravariance, return covariance, adding vs dropping a return type, all three arity cases, narrowed visibility, changed staticness, overriding `final`, and ADDING a by-ref return. Only one diverged, and it is worth recording so nobody re-audits the other thirteen: a subclass could DROP the parent method's `&` and nothing said so. php refuses that with `Fatal error: Declaration of C::f() must be compatible with & P::f()` -- the `&` prefixes the PARENT signature in the message, which is how php names the side that wanted it.\n\nThe root cause is a duplication that never happened: the one-directional by-ref rule (an override may ADD `&`, never DROP one) was written in `class_method_satisfies_interface_signature_with_return_mode` (interpreter/statements/interface_contracts.rs) for the INTERFACE path, and `class_method_signature_accepts` (interpreter/statements/interface_member_validation.rs) -- the PARENT-CLASS path, one file away and structurally parallel -- simply had no such check. The fix factors it into `override_by_ref_return_is_compatible(override_by_ref, required_by_ref)` called from both, rather than writing it a second time.\n\nphp's exact refusal wordings for this family, all measured:\n- `Declaration of C::f() must be compatible with & P::f()` (dropped by-ref)\n- `Declaration of C3::f(int $a) must be compatible with P3::f(mixed $a)` (narrowed parameter)\n- `Declaration of C5::f(): mixed must be compatible with P5::f(): int` (widened return)\n- `Declaration of C8::f() must be compatible with P8::f(): int` (dropped return type)\n- `Declaration of C9::f($a) must be compatible with P9::f($a, $b)` (fewer parameters)\n- `Declaration of CB::f($a, $b) must be compatible with PB::f($a)` (extra REQUIRED parameter; an extra parameter WITH a default is accepted)\n- `Access level to CC::f() must be public (as in class PC)` (narrowed visibility)\n- `Cannot make non static method PD::f() static in class CD`\n- `Cannot override final method PE::f()`\n\nMethod for this kind of audit: write a throwaway `#[ignore]` probe test that runs every shape through the interpreter and prints one line each, and a php script that runs the same shapes through `php -n -r` via shell_exec with the `in Command line code on line N` suffix stripped. Diff the two tables. That located a single one-line gap in a surface that would otherwise have looked like a large piece of work.\nEvidence: Seven tests in crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs covering all fourteen shapes, each expectation measured with php -n 8.5.6. Sentinel: with the by-ref rule removed from the parent path, exactly the one test that measures it fails and the other thirteen shapes stay green.\nVerified by: cargo test -p elephc-magician --lib &lt; /dev/null -- 1478 passed, 0 failed, 0 filtered. Commit 9fc72f502b.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval-interpreter","inheritance","override-compatibility","by-ref-return","php-conformance"],"paths":["crates/elephc-magician/src/interpreter/statements/interface_member_validation.rs","crates/elephc-magician/src/interpreter/statements/interface_contracts.rs","crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T13:12:46.797Z"}],"context":{"fact":"Fourteen override shapes were measured against `php -n` 8.5.6 and against the eval interpreter. THIRTEEN already agreed -- parameter contravariance, return covariance, adding vs dropping a return type, all three arity cases, narrowed visibility, changed staticness, overriding `final`, and ADDING a by-ref return. Only one diverged, and it is worth recording so nobody re-audits the other thirteen: a subclass could DROP the parent method's `&` and nothing said so. php refuses that with `Fatal error: Declaration of C::f() must be compatible with & P::f()` -- the `&` prefixes the PARENT signature in the message, which is how php names the side that wanted it.","verification":"Seven tests in crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs covering all fourteen shapes, each expectation measured with php -n 8.5.6. Sentinel: with the by-ref rule removed from the parent path, exactly the one test that measures it fails and the other thirteen shapes stay green."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T13:12:46.797Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/statements/interface_member_validation.rs","sha256":"41082f26cfd0ebce5d4727aba08f5900012be8eef2f3da8198f98ced66d6bf87","size":12915},{"path":"crates/elephc-magician/src/interpreter/statements/interface_contracts.rs","sha256":"5fc58e9a2f11d447bcaed78f244f523440a1e93e368bdd475d8f209a94861c39","size":27026}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":90000,"discovery_tokens_estimated":false,"score":72,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":["some referenced paths are missing: crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs"],"duplicate_candidates":[],"stale_reasons":["some referenced paths are missing: crates/elephc-magician/src/interpreter/tests/parent_override_compatibility.rs"],"estimated_tokens_saved":739},"created_at":"2026-09-06T13:12:46.797Z","updated_at":"2026-09-06T13:12:46.797Z","author_branch":"reconcile/dirname-symfony"}
```

