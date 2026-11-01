---
type: "Bug Fix"
title: "One walker, two positions: an instanceof guard was rejecting the constructor's parentheses"
description: "A dynamic class name after new was refused at the opening parenthesis, and the cause was a guard doing its job in the wrong place rather than a missing rule. parse variable class name target in parser/expressions/postfix"
resource: "crates/elephc-magician/src/parser/expressions/postfix.rs"
tags: ["session-learning", "eval-parser", "new-expression", "dynamic-class-name", "instanceof", "symfony-parse-sweep"]
timestamp: "2026-09-06T12:07:57.753Z"
x-kage-id: "repo:lazy-petting-popcorn:bug_fix:one-walker-two-positions-an-instanceof-guard-was-rejecting-the-constructors-pare"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/parser/expressions/postfix.rs", "crates/elephc-magician/src/parser/expressions/static_names.rs"]
x-kage-stack: ["rust", "php"]
---

# One walker, two positions: an instanceof guard was rejecting the constructor's parentheses

> A dynamic class name after new was refused at the opening parenthesis, and the cause was a guard doing its job in the…

A dynamic class name after new was refused at the opening parenthesis, and the cause was a guard doing its job in the wrong place rather than a missing rule. parse_variable_class_name_target in parser/expressions/postfix.rs walks a class name through index and property steps, and it refuses a parenthesis after a property read so that an instanceof operand cannot be read as a method call — PHP's grammar does not allow one there, and accepting it would silently rename the operand. After new, that same parenthesis opens the CONSTRUCTOR argument list. When one walker serves two syntactic positions, a guard that is correct in one is a refusal in the other; pass the position in rather than choosing a side.

Reading a STATIC property through an object as a class name was a missing step in the walker, in BOTH positions, not a flag: the walker knew index and property steps and had no double-colon step at all.

TEST DESIGN worth reusing for constructor shapes: give each shape a DIFFERENT constructor argument. A lowering that resolves the right class but drops or shuffles the arguments passes every shape otherwise, because the class name is all the assertion looks at.

Also pinned here because they share the path and are easy to assume: the relative keywords, where static follows the CALLED class and self the DECLARING one — that difference is the whole reason both exist, and a test using only one of them proves nothing about the other.
Evidence: php -n 8.5.6 prints d;m;a, s, p;c and Child;Base;Base on the four fragments and elephc now prints the same. Sentinels: applying the call guard in constructor position again fails exactly the property and element test; disabling the static-property step fails exactly the static-property test. Symfony measurement: the sweep's thirteen refused files reach ZERO, having gone 13 to 8 to 6 to 5 to 3 to 1 to 0 across the series.
Verified by: cargo test -p elephc-magician --lib with no skips: 1437 passed, 0 failed, 0 filtered (commit 37a2650457)

## Verification

php -n 8.5.6 prints d;m;a, s, p;c and Child;Base;Base on the four fragments and elephc now prints the same. Sentinels: applying the call guard in constructor position again fails exactly the property and element test; disabling the static-property step fails exactly the static-property test. Symfony measurement: the sweep's thirteen refused files reach ZERO, having gone 13 to 8 to 6 to 5 to 3 to 1 to 0 across the series.

# Citations

[1] explicit_capture (2026-09-06T12:07:57.753Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:bug_fix:one-walker-two-positions-an-instanceof-guard-was-rejecting-the-constructors-pare","title":"One walker, two positions: an instanceof guard was rejecting the constructor's parentheses","summary":"A dynamic class name after new was refused at the opening parenthesis, and the cause was a guard doing its job in the wrong place rather than a missing rule. parse variable class name target in parser/expressions/postfix","body":"A dynamic class name after new was refused at the opening parenthesis, and the cause was a guard doing its job in the wrong place rather than a missing rule. parse_variable_class_name_target in parser/expressions/postfix.rs walks a class name through index and property steps, and it refuses a parenthesis after a property read so that an instanceof operand cannot be read as a method call — PHP's grammar does not allow one there, and accepting it would silently rename the operand. After new, that same parenthesis opens the CONSTRUCTOR argument list. When one walker serves two syntactic positions, a guard that is correct in one is a refusal in the other; pass the position in rather than choosing a side.\n\nReading a STATIC property through an object as a class name was a missing step in the walker, in BOTH positions, not a flag: the walker knew index and property steps and had no double-colon step at all.\n\nTEST DESIGN worth reusing for constructor shapes: give each shape a DIFFERENT constructor argument. A lowering that resolves the right class but drops or shuffles the arguments passes every shape otherwise, because the class name is all the assertion looks at.\n\nAlso pinned here because they share the path and are easy to assume: the relative keywords, where static follows the CALLED class and self the DECLARING one — that difference is the whole reason both exist, and a test using only one of them proves nothing about the other.\nEvidence: php -n 8.5.6 prints d;m;a, s, p;c and Child;Base;Base on the four fragments and elephc now prints the same. Sentinels: applying the call guard in constructor position again fails exactly the property and element test; disabling the static-property step fails exactly the static-property test. Symfony measurement: the sweep's thirteen refused files reach ZERO, having gone 13 to 8 to 6 to 5 to 3 to 1 to 0 across the series.\nVerified by: cargo test -p elephc-magician --lib with no skips: 1437 passed, 0 failed, 0 filtered (commit 37a2650457)","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval-parser","new-expression","dynamic-class-name","instanceof","symfony-parse-sweep"],"paths":["crates/elephc-magician/src/parser/expressions/postfix.rs","crates/elephc-magician/src/parser/expressions/static_names.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T12:07:57.753Z"}],"context":{"fact":"A dynamic class name after new was refused at the opening parenthesis, and the cause was a guard doing its job in the wrong place rather than a missing rule. parse_variable_class_name_target in parser/expressions/postfix.rs walks a class name through index and property steps, and it refuses a parenthesis after a property read so that an instanceof operand cannot be read as a method call — PHP's grammar does not allow one there, and accepting it would silently rename the operand. After new, that same parenthesis opens the CONSTRUCTOR argument list. When one walker serves two syntactic positions, a guard that is correct in one is a refusal in the other; pass the position in rather than choosing a side.","verification":"php -n 8.5.6 prints d;m;a, s, p;c and Child;Base;Base on the four fragments and elephc now prints the same. Sentinels: applying the call guard in constructor position again fails exactly the property and element test; disabling the static-property step fails exactly the static-property test. Symfony measurement: the sweep's thirteen refused files reach ZERO, having gone 13 to 8 to 6 to 5 to 3 to 1 to 0 across the series."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T12:07:57.753Z","path_fingerprints":[{"path":"crates/elephc-magician/src/parser/expressions/postfix.rs","sha256":"eb119682dc1678428f22b5a917bd7d1dd10006a302dce876248d3f0d7910f280","size":10903},{"path":"crates/elephc-magician/src/parser/expressions/static_names.rs","sha256":"9d795912bcaf0c0a695529672af4ce052d9ea99a3a26e107d1e31bfc25473847","size":15128}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":30000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":501},"created_at":"2026-09-06T12:07:57.753Z","updated_at":"2026-09-06T12:07:57.753Z","author_branch":"reconcile/dirname-symfony"}
```

