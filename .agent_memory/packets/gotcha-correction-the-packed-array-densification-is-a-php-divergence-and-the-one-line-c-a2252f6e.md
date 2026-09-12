---
type: "Gotcha"
title: "CORRECTION: the packed-array densification is a PHP divergence, and the one-line checker fix is partial AND regressive"
description: "Corrects the earlier packet on this bug, which called it a \"design call\" and claimed the simple fix would cost performance. Both framings were wrong, and the second was asserted without measuring. IT IS A DIVERGENCE, not"
resource: "src/types/array_keys.rs"
tags: ["session-learning", "arrays", "packed-array", "checker", "open-bug", "php-semantics", "correction"]
timestamp: "2026-09-11T19:23:51.919Z"
x-kage-id: "repo:sparkling-jingling-flute:gotcha:correction-the-packed-array-densification-is-a-php-divergence-and-the-one-line-c"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/types/array_keys.rs", "src/types/checker/stmt_check/assignments/arrays.rs", "src/codegen/lower_inst/arrays.rs"]
x-kage-stack: ["rust", "php"]
---

# CORRECTION: the packed-array densification is a PHP divergence, and the one-line checker fix is partial AND regressive

> Corrects the earlier packet on this bug, which called it a "design call" and claimed the simple fix would cost perfor…

Corrects the earlier packet on this bug, which called it a "design call" and claimed the simple fix would cost performance. Both framings were wrong, and the second was asserted without measuring.

IT IS A DIVERGENCE, not a tradeoff. `$k = 5; $a = []; $a[$k] = 3;` prints six elements where `php -n` prints one. Wrong output, silently, from a compiler whose contract is PHP compatibility. The cost of a fix does not make the defect optional.

THE ONE-LINE FIX DOES NOT WORK. Flipping `static_array_key_forces_hash_storage`'s `_ => false` to `_ => true` (`src/types/array_keys.rs:58`) was measured on four shapes:

| shape | before | after `_ => true` |
|---|---|---|
| `$k=5; $a=[]; $a[$k]=3` | DIVERGES | FIXED |
| `$k=5; $a=[1]; $a[$k]=3` | DIVERGES | still DIVERGES |
| `for ($i…) $a[$i*2]=$i` (sparse keys) | DIVERGES | still DIVERGES |
| `function w(array $a, int $k): array { $a[$k]=3; return $a; }` | DIVERGES (compiles) | **COMPILE REGRESSION** |

The regression is the decisive one: promoting a DECLARED `array` parameter to `AssocArray` breaks the EIR return boundary — "unsupported EIR backend feature: runtime_call from PHP type Array(Int) to PHP type AssocArray { key: Int, value: Int }". A program that ran (wrongly) now does not build.

WHY IT IS PARTIAL: the clause is gated on the array still being EMPTY —
`matches!(elem_ty.as_ref(), PhpType::Never) && static_array_key_forces_hash_storage(index)`
(`src/types/checker/stmt_check/assignments/arrays.rs:70`). It never sees a write into an array that already has an element type, which is why the non-empty and loop shapes are untouched.

THE PERFORMANCE CLAIM WAS FALSE. A 50k-element sequential build measured 5.1/5.1/4.9 ms before and 5.1/4.9/4.9 ms after — indistinguishable. The EIR shows why: the loop still emits `array_set` on `array<never>`-turned-`array<int>` storage, because the checker widens the element type before the write is checked, so the clause never fires there. "No cost" meant "no change on that path", not "the optimization is free".

WHAT THE REAL FIX NEEDS: the promotion has to be decidable for a write into an array that already has an element type, and it has to survive a declared `array` return/parameter boundary. `__rt_array_set_mixed_key` already promotes STRING keys at runtime and writes the new pointer back to the local, so the runtime machinery exists; what blocks the integer case is the static type map, and `Effects::MAY_DEOPT` is a flag with no codegen implementation to hang a representation change on.
Evidence: Four probes diffed against `php -n` 8.5.6 with the change applied and reverted; a 50k sequential-build benchmark run three times in each state; EIR dumps confirming the loop path still emits `array_set`.
Verified by: The change was applied, measured, shown to regress `q4_param` into a compile failure, and reverted. The tree is back at the committed state.

## Why

the loop still emits `array_set` on `array<never>`-turned-`array<int>` storage, because the checker widens the element type before the write is checked, so the clause never fires there. "No cost" meant "no change on that path", not "the optimization is free".

WHAT THE REAL FIX NEEDS: the promotion has to be decidable for a write into an array that already has an element type, and it has to survive a declared `array` return/parameter boundary. `__rt_array_set_mixed_key` already promotes STRING keys at runtime and writes the new pointer back to the local, so the runtime machinery exists; what blocks the integer case is the static type map, and `Effects::MAY_DEOPT` is a flag with no codegen implementation to hang a representation change on.

## Verification

Four probes diffed against `php -n` 8.5.6 with the change applied and reverted; a 50k sequential-build benchmark run three times in each state; EIR dumps confirming the loop path still emits `array_set`.

# Citations

[1] explicit_capture (2026-09-11T19:23:51.919Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:gotcha:correction-the-packed-array-densification-is-a-php-divergence-and-the-one-line-c","title":"CORRECTION: the packed-array densification is a PHP divergence, and the one-line checker fix is partial AND regressive","summary":"Corrects the earlier packet on this bug, which called it a \"design call\" and claimed the simple fix would cost performance. Both framings were wrong, and the second was asserted without measuring. IT IS A DIVERGENCE, not","body":"Corrects the earlier packet on this bug, which called it a \"design call\" and claimed the simple fix would cost performance. Both framings were wrong, and the second was asserted without measuring.\n\nIT IS A DIVERGENCE, not a tradeoff. `$k = 5; $a = []; $a[$k] = 3;` prints six elements where `php -n` prints one. Wrong output, silently, from a compiler whose contract is PHP compatibility. The cost of a fix does not make the defect optional.\n\nTHE ONE-LINE FIX DOES NOT WORK. Flipping `static_array_key_forces_hash_storage`'s `_ => false` to `_ => true` (`src/types/array_keys.rs:58`) was measured on four shapes:\n\n| shape | before | after `_ => true` |\n|---|---|---|\n| `$k=5; $a=[]; $a[$k]=3` | DIVERGES | FIXED |\n| `$k=5; $a=[1]; $a[$k]=3` | DIVERGES | still DIVERGES |\n| `for ($i…) $a[$i*2]=$i` (sparse keys) | DIVERGES | still DIVERGES |\n| `function w(array $a, int $k): array { $a[$k]=3; return $a; }` | DIVERGES (compiles) | **COMPILE REGRESSION** |\n\nThe regression is the decisive one: promoting a DECLARED `array` parameter to `AssocArray` breaks the EIR return boundary — \"unsupported EIR backend feature: runtime_call from PHP type Array(Int) to PHP type AssocArray { key: Int, value: Int }\". A program that ran (wrongly) now does not build.\n\nWHY IT IS PARTIAL: the clause is gated on the array still being EMPTY —\n`matches!(elem_ty.as_ref(), PhpType::Never) && static_array_key_forces_hash_storage(index)`\n(`src/types/checker/stmt_check/assignments/arrays.rs:70`). It never sees a write into an array that already has an element type, which is why the non-empty and loop shapes are untouched.\n\nTHE PERFORMANCE CLAIM WAS FALSE. A 50k-element sequential build measured 5.1/5.1/4.9 ms before and 5.1/4.9/4.9 ms after — indistinguishable. The EIR shows why: the loop still emits `array_set` on `array<never>`-turned-`array<int>` storage, because the checker widens the element type before the write is checked, so the clause never fires there. \"No cost\" meant \"no change on that path\", not \"the optimization is free\".\n\nWHAT THE REAL FIX NEEDS: the promotion has to be decidable for a write into an array that already has an element type, and it has to survive a declared `array` return/parameter boundary. `__rt_array_set_mixed_key` already promotes STRING keys at runtime and writes the new pointer back to the local, so the runtime machinery exists; what blocks the integer case is the static type map, and `Effects::MAY_DEOPT` is a flag with no codegen implementation to hang a representation change on.\nEvidence: Four probes diffed against `php -n` 8.5.6 with the change applied and reverted; a 50k sequential-build benchmark run three times in each state; EIR dumps confirming the loop path still emits `array_set`.\nVerified by: The change was applied, measured, shown to regress `q4_param` into a compile failure, and reverted. The tree is back at the committed state.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","arrays","packed-array","checker","open-bug","php-semantics","correction"],"paths":["src/types/array_keys.rs","src/types/checker/stmt_check/assignments/arrays.rs","src/codegen/lower_inst/arrays.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T19:23:51.919Z"}],"context":{"fact":"Corrects the earlier packet on this bug, which called it a \"design call\" and claimed the simple fix would cost performance. Both framings were wrong, and the second was asserted without measuring.","why":"the loop still emits `array_set` on `array<never>`-turned-`array<int>` storage, because the checker widens the element type before the write is checked, so the clause never fires there. \"No cost\" meant \"no change on that path\", not \"the optimization is free\".\n\nWHAT THE REAL FIX NEEDS: the promotion has to be decidable for a write into an array that already has an element type, and it has to survive a declared `array` return/parameter boundary. `__rt_array_set_mixed_key` already promotes STRING keys at runtime and writes the new pointer back to the local, so the runtime machinery exists; what blocks the integer case is the static type map, and `Effects::MAY_DEOPT` is a flag with no codegen implementation to hang a representation change on.","verification":"Four probes diffed against `php -n` 8.5.6 with the change applied and reverted; a 50k sequential-build benchmark run three times in each state; EIR dumps confirming the loop path still emits `array_set`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T19:23:51.919Z","path_fingerprints":[{"path":"src/types/array_keys.rs","sha256":"3885c2c8f1f9083180eec3f2d728e91c766d95708d00856033e7ed4bbb896a38","size":6417},{"path":"src/types/checker/stmt_check/assignments/arrays.rs","sha256":"90f64e1c51065f8c1c9c862c1b8c358a147f45e3ad7d183e12aab21b1fd4d7ce","size":10909},{"path":"src/codegen/lower_inst/arrays.rs","sha256":"67cd2f80fbf2cde04cbbcae35a4674a3fc008c665f580c71fc2b80616d2c46ed","size":143233}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":720},"created_at":"2026-09-11T19:23:51.919Z","updated_at":"2026-09-12T16:41:20.952Z","author_branch":"feat/opcache-runtime-cache"}
```

