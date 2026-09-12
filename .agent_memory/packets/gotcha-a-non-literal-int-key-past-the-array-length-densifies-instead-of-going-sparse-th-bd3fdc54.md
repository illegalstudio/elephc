---
type: "Gotcha"
title: "A non-literal int key past the array length densifies instead of going sparse: the packed-array optimization is unsound out of range"
description: "$k = 5; $a = ; $a $k = 3; produces a SIX element array 0..4 = int 0 , 5 = 3 where PHP produces one element. The literal form $a 5 = 3 is correct. ROOT CAUSE, exact and deliberate: static array key forces hash storage src"
resource: "src/types/array_keys.rs"
tags: ["session-learning", "arrays", "packed-array", "checker", "open-bug", "php-semantics"]
timestamp: "2026-09-11T19:15:32.581Z"
x-kage-id: "repo:sparkling-jingling-flute:gotcha:a-non-literal-int-key-past-the-array-length-densifies-instead-of-going-sparse-th"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/types/array_keys.rs", "src/codegen/lower_inst/arrays.rs", "src/ir/effects.rs"]
x-kage-stack: ["rust", "php"]
---

# A non-literal int key past the array length densifies instead of going sparse: the packed-array optimization is unsound out of range

> $k = 5; $a = ; $a $k = 3; produces a SIX element array 0..4 = int 0 , 5 = 3 where PHP produces one element. The liter…

`$k = 5; $a = []; $a[$k] = 3;` produces a SIX-element array (`[0..4] => int(0)`, `[5] => 3`) where PHP produces one element. The literal form `$a[5] = 3` is correct.

ROOT CAUSE, exact and deliberate: `static_array_key_forces_hash_storage` (`src/types/array_keys.rs:58`) answers `true` for a non-zero integer LITERAL and `_ => false` for everything else, with the docblock stating the intent — "Other expressions (variables, function calls, etc.) do not force hash storage and may use packed array optimization." That optimization is unsound whenever the key lands past the array's length: PHP makes the array sparse, elephc's `array_set` grows the packed storage and zero-fills the gap.

MEASURED SHAPE (one probe per case, against `php -n`):
| write | result |
|---|---|
| `$a[5] = 3` (literal) | correct — checker emits `array_to_hash` then `hash_set` |
| `$k = 5; $a[$k] = 3` | DENSIFIES |
| `$k = 1; $a = []; $a[$k] = 3` | DENSIFIES |
| `$k = 0; $a = []; $a[$k] = 3` | correct (key 0 is the append slot) |
| `$k = 5; $a = [1]; $a[$k] = 3` | DENSIFIES |
| runtime-unknown key, and an `int $k` PARAMETER | DENSIFIES |

Constant folding does NOT rescue it: the EIR for `$k = 1` already shows `const_i64 1`, yet the local is typed `Heap(Array) php=array<never>` and the write is `array_set`, while the literal form types the local `Heap(Hash)` and emits `hash_set`. The decision is made in the checker from the SOURCE form, before folding.

THREE FIXES, and the choice is architectural rather than a bug fix:
1. `_ => true` — always hash for a non-literal key. Correct and small, but removes the packed representation from `for ($i = 0; $i < $n; $i++) { $a[$i] = …; }`, the most common array build in PHP.
2. Promote at RUNTIME when the index exceeds the length, which is what PHP does. BLOCKED: the value's static type is `Heap(Array)` and downstream consumers are compiled against it (`load_local` → `var_dump` expects an Array), so a representation change mid-flight needs type reconciliation. `Effects::MAY_DEOPT` exists as a FLAG ONLY — `grep deopt src/codegen src/codegen_support` finds no implementation — so there is no existing deopt path to hang this on.
3. Prove sequentiality where possible (a range/monotonicity analysis over the loop induction variable) and fall back to hash otherwise. Keeps the optimization where it is sound. The most work.

Related and already filed: `__rt_array_set_mixed_key` already promotes STRING keys to a hash at runtime and writes the new pointer back to the local, so the write-back machinery exists — it is the static type map, not the runtime, that blocks extending it to integer keys.
Evidence: Eight probes compiled and diffed against `php -n` 8.5.6, plus the EIR dumps for the literal and non-literal forms side by side (`array_to_hash`/`hash_set` versus `array_set` on `array<never>`).
Verified by: Reproduced on macOS arm64 with the branch compiler. NOT fixed: the three candidate fixes trade correctness against the packed-array representation, which is a design call rather than a defect to patch.

## Verification

Eight probes compiled and diffed against `php -n` 8.5.6, plus the EIR dumps for the literal and non-literal forms side by side (`array_to_hash`/`hash_set` versus `array_set` on `array<never>`).

# Citations

[1] explicit_capture (2026-09-11T19:15:32.581Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:gotcha:a-non-literal-int-key-past-the-array-length-densifies-instead-of-going-sparse-th","title":"A non-literal int key past the array length densifies instead of going sparse: the packed-array optimization is unsound out of range","summary":"$k = 5; $a = ; $a $k = 3; produces a SIX element array 0..4 = int 0 , 5 = 3 where PHP produces one element. The literal form $a 5 = 3 is correct. ROOT CAUSE, exact and deliberate: static array key forces hash storage src","body":"`$k = 5; $a = []; $a[$k] = 3;` produces a SIX-element array (`[0..4] => int(0)`, `[5] => 3`) where PHP produces one element. The literal form `$a[5] = 3` is correct.\n\nROOT CAUSE, exact and deliberate: `static_array_key_forces_hash_storage` (`src/types/array_keys.rs:58`) answers `true` for a non-zero integer LITERAL and `_ => false` for everything else, with the docblock stating the intent — \"Other expressions (variables, function calls, etc.) do not force hash storage and may use packed array optimization.\" That optimization is unsound whenever the key lands past the array's length: PHP makes the array sparse, elephc's `array_set` grows the packed storage and zero-fills the gap.\n\nMEASURED SHAPE (one probe per case, against `php -n`):\n| write | result |\n|---|---|\n| `$a[5] = 3` (literal) | correct — checker emits `array_to_hash` then `hash_set` |\n| `$k = 5; $a[$k] = 3` | DENSIFIES |\n| `$k = 1; $a = []; $a[$k] = 3` | DENSIFIES |\n| `$k = 0; $a = []; $a[$k] = 3` | correct (key 0 is the append slot) |\n| `$k = 5; $a = [1]; $a[$k] = 3` | DENSIFIES |\n| runtime-unknown key, and an `int $k` PARAMETER | DENSIFIES |\n\nConstant folding does NOT rescue it: the EIR for `$k = 1` already shows `const_i64 1`, yet the local is typed `Heap(Array) php=array<never>` and the write is `array_set`, while the literal form types the local `Heap(Hash)` and emits `hash_set`. The decision is made in the checker from the SOURCE form, before folding.\n\nTHREE FIXES, and the choice is architectural rather than a bug fix:\n1. `_ => true` — always hash for a non-literal key. Correct and small, but removes the packed representation from `for ($i = 0; $i < $n; $i++) { $a[$i] = …; }`, the most common array build in PHP.\n2. Promote at RUNTIME when the index exceeds the length, which is what PHP does. BLOCKED: the value's static type is `Heap(Array)` and downstream consumers are compiled against it (`load_local` → `var_dump` expects an Array), so a representation change mid-flight needs type reconciliation. `Effects::MAY_DEOPT` exists as a FLAG ONLY — `grep deopt src/codegen src/codegen_support` finds no implementation — so there is no existing deopt path to hang this on.\n3. Prove sequentiality where possible (a range/monotonicity analysis over the loop induction variable) and fall back to hash otherwise. Keeps the optimization where it is sound. The most work.\n\nRelated and already filed: `__rt_array_set_mixed_key` already promotes STRING keys to a hash at runtime and writes the new pointer back to the local, so the write-back machinery exists — it is the static type map, not the runtime, that blocks extending it to integer keys.\nEvidence: Eight probes compiled and diffed against `php -n` 8.5.6, plus the EIR dumps for the literal and non-literal forms side by side (`array_to_hash`/`hash_set` versus `array_set` on `array<never>`).\nVerified by: Reproduced on macOS arm64 with the branch compiler. NOT fixed: the three candidate fixes trade correctness against the packed-array representation, which is a design call rather than a defect to patch.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","arrays","packed-array","checker","open-bug","php-semantics"],"paths":["src/types/array_keys.rs","src/codegen/lower_inst/arrays.rs","src/ir/effects.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T19:15:32.581Z"}],"context":{"fact":"`$k = 5; $a = []; $a[$k] = 3;` produces a SIX-element array (`[0..4] => int(0)`, `[5] => 3`) where PHP produces one element. The literal form `$a[5] = 3` is correct.","verification":"Eight probes compiled and diffed against `php -n` 8.5.6, plus the EIR dumps for the literal and non-literal forms side by side (`array_to_hash`/`hash_set` versus `array_set` on `array<never>`)."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T19:15:32.581Z","path_fingerprints":[{"path":"src/types/array_keys.rs","sha256":"3885c2c8f1f9083180eec3f2d728e91c766d95708d00856033e7ed4bbb896a38","size":6417},{"path":"src/codegen/lower_inst/arrays.rs","sha256":"67cd2f80fbf2cde04cbbcae35a4674a3fc008c665f580c71fc2b80616d2c46ed","size":143233},{"path":"src/ir/effects.rs","sha256":"579db6199a3eff413f546a2218ea49b8d641240c9196918eaa31a1493183d7fa","size":4158}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":763},"created_at":"2026-09-11T19:15:32.581Z","updated_at":"2026-09-12T16:41:20.951Z","author_branch":"feat/opcache-runtime-cache"}
```

