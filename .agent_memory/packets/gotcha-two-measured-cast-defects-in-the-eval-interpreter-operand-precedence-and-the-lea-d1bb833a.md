---
type: "Gotcha"
title: "Two measured cast defects in the eval interpreter: operand precedence and the leading-numeric int cast"
description: "Found while adding the object cast, measured but NOT fixed, and each is its own construct rather than part of that work. Both are silent wrong answers rather than refusals, which is why they are worth recording. FIRST, c"
resource: "crates/elephc-magician/src/parser/expressions/precedence.rs"
tags: ["session-learning", "eval-parser", "casts", "operator-precedence", "numeric-strings", "open-defect"]
timestamp: "2026-09-06T09:34:07.913Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:two-measured-cast-defects-in-the-eval-interpreter-operand-precedence-and-the-lea"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/parser/expressions/precedence.rs", "crates/elephc-magician/src/interpreter/expressions/evaluation.rs", "crates/elephc-magician/src/parser/tests/operators.rs"]
x-kage-stack: ["rust", "php"]
---

# Two measured cast defects in the eval interpreter: operand precedence and the leading-numeric int cast

> Found while adding the object cast, measured but NOT fixed, and each is its own construct rather than part of that wo…

Found while adding the object cast, measured but NOT fixed, and each is its own construct rather than part of that work. Both are silent wrong answers rather than refusals, which is why they are worth recording.

FIRST, cast operand precedence. parse_unary in crates/elephc-magician/src/parser/expressions/precedence.rs parses a cast operand with parse_concat, so the cast swallows the whole concatenation chain: a cast of a variable followed by a dot and a string parses as a cast OF the concatenation. In PHP a cast binds tighter than the dot, at the same level as the other unary operators. Measured: php -n 8.5.6 on an int cast of the string 3 concatenated with 4x prints 34x; elephc prints 0. The existing parser test named parse_fragment_accepts_scalar_cast_source PINS the wrong shape and its docblock claims PHP cast precedence, so a reader checking the tests would conclude this was already right. Fixing it means parsing the operand with parse_unary instead, and re-pinning that test.

SECOND, the int cast of a leading-numeric string. php gives 34 for an int cast of the string 34x and 0 for x34 -- it takes the longest numeric prefix. elephc gives 0 for both. That one is in the runtime cast_int hook rather than the parser, and it compounds with the first defect: with correct precedence the concatenation case would print 34, still not php's 34x, so fixing either alone leaves a wrong answer.

The two together are why the concatenation probe prints 0 rather than 34: the cast takes the whole concatenation AND then the numeric-prefix rule is missing. A test that only checked the parenthesised spelling passes, because that spelling is correct today.
Evidence: php -n 8.5.6: an int cast of the string 3 concatenated with 4x prints 34x, and var_dump of an int cast of 34x is int(34) while x34 is int(0). elephc printed 0 for the concatenation case and 0 for the direct cast of 34x, measured through an ignored probe test run against FakeOps and then removed. The parenthesised spelling printed 34x, which isolates the precedence half.
Verified by: Throwaway ignored test run on commit 2f5f3e820c, output CASTPREC=0;34x against php -n 8.5.6's 34 and 34x

## Verification

php -n 8.5.6: an int cast of the string 3 concatenated with 4x prints 34x, and var_dump of an int cast of 34x is int(34) while x34 is int(0). elephc printed 0 for the concatenation case and 0 for the direct cast of 34x, measured through an ignored probe test run against FakeOps and then removed. The parenthesised spelling printed 34x, which isolates the precedence half.

# Citations

[1] explicit_capture (2026-09-06T09:34:07.913Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:two-measured-cast-defects-in-the-eval-interpreter-operand-precedence-and-the-lea","title":"Two measured cast defects in the eval interpreter: operand precedence and the leading-numeric int cast","summary":"Found while adding the object cast, measured but NOT fixed, and each is its own construct rather than part of that work. Both are silent wrong answers rather than refusals, which is why they are worth recording. FIRST, c","body":"Found while adding the object cast, measured but NOT fixed, and each is its own construct rather than part of that work. Both are silent wrong answers rather than refusals, which is why they are worth recording.\n\nFIRST, cast operand precedence. parse_unary in crates/elephc-magician/src/parser/expressions/precedence.rs parses a cast operand with parse_concat, so the cast swallows the whole concatenation chain: a cast of a variable followed by a dot and a string parses as a cast OF the concatenation. In PHP a cast binds tighter than the dot, at the same level as the other unary operators. Measured: php -n 8.5.6 on an int cast of the string 3 concatenated with 4x prints 34x; elephc prints 0. The existing parser test named parse_fragment_accepts_scalar_cast_source PINS the wrong shape and its docblock claims PHP cast precedence, so a reader checking the tests would conclude this was already right. Fixing it means parsing the operand with parse_unary instead, and re-pinning that test.\n\nSECOND, the int cast of a leading-numeric string. php gives 34 for an int cast of the string 34x and 0 for x34 -- it takes the longest numeric prefix. elephc gives 0 for both. That one is in the runtime cast_int hook rather than the parser, and it compounds with the first defect: with correct precedence the concatenation case would print 34, still not php's 34x, so fixing either alone leaves a wrong answer.\n\nThe two together are why the concatenation probe prints 0 rather than 34: the cast takes the whole concatenation AND then the numeric-prefix rule is missing. A test that only checked the parenthesised spelling passes, because that spelling is correct today.\nEvidence: php -n 8.5.6: an int cast of the string 3 concatenated with 4x prints 34x, and var_dump of an int cast of 34x is int(34) while x34 is int(0). elephc printed 0 for the concatenation case and 0 for the direct cast of 34x, measured through an ignored probe test run against FakeOps and then removed. The parenthesised spelling printed 34x, which isolates the precedence half.\nVerified by: Throwaway ignored test run on commit 2f5f3e820c, output CASTPREC=0;34x against php -n 8.5.6's 34 and 34x","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval-parser","casts","operator-precedence","numeric-strings","open-defect"],"paths":["crates/elephc-magician/src/parser/expressions/precedence.rs","crates/elephc-magician/src/interpreter/expressions/evaluation.rs","crates/elephc-magician/src/parser/tests/operators.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T09:34:07.913Z"}],"context":{"fact":"Found while adding the object cast, measured but NOT fixed, and each is its own construct rather than part of that work. Both are silent wrong answers rather than refusals, which is why they are worth recording.","verification":"php -n 8.5.6: an int cast of the string 3 concatenated with 4x prints 34x, and var_dump of an int cast of 34x is int(34) while x34 is int(0). elephc printed 0 for the concatenation case and 0 for the direct cast of 34x, measured through an ignored probe test run against FakeOps and then removed. The parenthesised spelling printed 34x, which isolates the precedence half."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T09:34:07.913Z","path_fingerprints":[{"path":"crates/elephc-magician/src/parser/expressions/precedence.rs","sha256":"946842911014203cf0ee91ba0f27826dc870353f3115d73c710f252c2b54c965","size":30692},{"path":"crates/elephc-magician/src/interpreter/expressions/evaluation.rs","sha256":"67b9244c7f61d4156840ad50a733bad3dbf21433d6e997957812e325c2175531","size":28579},{"path":"crates/elephc-magician/src/parser/tests/operators.rs","sha256":"968b5c768a5626a63ee8a3def7a9dff339d8a0e4a28865295be01fe6544918d6","size":18155}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":15000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":542},"created_at":"2026-09-06T09:34:07.913Z","updated_at":"2026-09-06T09:34:07.913Z","author_branch":"reconcile/dirname-symfony"}
```

