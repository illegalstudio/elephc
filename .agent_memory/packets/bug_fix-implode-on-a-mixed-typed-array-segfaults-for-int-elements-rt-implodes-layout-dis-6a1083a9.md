---
type: "Bug Fix"
title: "implode() on a mixed-typed array segfaults for int elements: __rt_implode's layout dispatch handles only two of them"
description: "implode SEGFAULTS on an array whose static type is Mixed when the runtime elements are ints. Four line reproducer: ROOT CAUSE, isolated by measurement rather than reading: implode runtime label src/codegen/lower inst/bui"
resource: "src/codegen_support/runtime/strings/implode.rs"
tags: ["session-learning", "implode", "mixed", "segfault", "runtime-helper", "open-bug"]
timestamp: "2026-09-11T17:04:11.160Z"
x-kage-id: "repo:sparkling-jingling-flute:bug_fix:implode-on-a-mixed-typed-array-segfaults-for-int-elements-rt-implodes-layout-dis"
x-kage-type: "bug_fix"
x-kage-status: "superseded"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "superseded"
x-kage-paths: ["src/codegen_support/runtime/strings/implode.rs", "src/codegen/lower_inst/builtins/strings/split.rs"]
x-kage-stack: ["rust", "php", "aarch64"]
---

# implode() on a mixed-typed array segfaults for int elements: __rt_implode's layout dispatch handles only two of them

> implode SEGFAULTS on an array whose static type is Mixed when the runtime elements are ints. Four line reproducer: RO…

`implode()` SEGFAULTS on an array whose static type is `Mixed` when the runtime elements are ints. Four-line reproducer:

```php
function f(): mixed { return [1, 2]; }
echo implode(",", (array) f());   // Segmentation fault (exit 139)
```

ROOT CAUSE, isolated by measurement rather than reading:
- `implode_runtime_label` (`src/codegen/lower_inst/builtins/strings/split.rs:567`) maps `PhpType::Mixed` to the generic `__rt_implode`. It cannot do better — the element type is unknown statically.
- `__rt_implode` (`src/codegen_support/runtime/strings/implode.rs`) dispatches on the array's own `value_type` tag, read from the packed metadata word at `[array_ptr - 8]`. It handles exactly TWO layouts: tag 7 (boxed Mixed cells, via `__rt_mixed_cast_string`) and everything else as `(ptr, len)` STRING PAIRS.
- An array of raw ints is neither. Each int is read as a string pointer and dereferenced.

The discriminating sweep — same shape, only the element type varying:
| mixed holds | result |
|---|---|
| `["a","b"]` (string elements) | IDENTICAL to php |
| `[1, 2]` (int elements) | SEGFAULT |
| `["x"=>1,"y"=>2]` (assoc, int values) | SEGFAULT |
| `[]` (empty) | IDENTICAL to php |

So it is the ELEMENT TYPE, not indexed-vs-hash. `count()`, `array_keys()` and `var_dump()` on the identical value all behave correctly, which is what rules out the cast's representation and points at this one helper.

SUGGESTED FIX SHAPE: complete the existing tag switch in `__rt_implode` — add the int arm (what `__rt_implode_int` already does through `__rt_itoa`) and the bool arm (`"1"`/`""`, NOT `"1"`/`"0"`, per `implode_element_runtime_label`'s comment). Doing it in the helper rather than at the lowering keeps one copy of the tag logic. ⚠️ The helper's docblock pins TWO invariants in the boxed-Mixed path — the owned mixed-cast slot (ownership, #601) and the live `_concat_off` cursor — and both must survive a rework; they are pinned by `tests/codegen/runtime_gc/regressions.rs` and `tests/array_result_type_tests.rs`.

Also found while probing: `(array)` cast of a value already statically typed `array` is REFUSED — "unsupported EIR backend feature: cast to EIR type Heap(Array)". And `array_sum()` on a `Mixed`-typed array is refused by the checker ("argument must be array").
Evidence: Five probes compiled and run against `php -n` 8.5.6, one file per case so a crash cannot mask the rest. Emitted assembly confirms the call site unboxes the Mixed and passes the payload in x3 to `__rt_implode`, so the renderer choice — not the argument — is what is wrong.
Verified by: Reproduced on macOS arm64 with the branch compiler; exit code 139 on the int-element shapes, byte-identical output on the string-element and empty shapes.

## Verification

Five probes compiled and run against `php -n` 8.5.6, one file per case so a crash cannot mask the rest. Emitted assembly confirms the call site unboxes the Mixed and passes the payload in x3 to `__rt_implode`, so the renderer choice — not the argument — is what is wrong.

# Citations

[1] explicit_capture (2026-09-11T17:04:11.160Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:bug_fix:implode-on-a-mixed-typed-array-segfaults-for-int-elements-rt-implodes-layout-dis","title":"implode() on a mixed-typed array segfaults for int elements: __rt_implode's layout dispatch handles only two of them","summary":"implode SEGFAULTS on an array whose static type is Mixed when the runtime elements are ints. Four line reproducer: ROOT CAUSE, isolated by measurement rather than reading: implode runtime label src/codegen/lower inst/bui","body":"`implode()` SEGFAULTS on an array whose static type is `Mixed` when the runtime elements are ints. Four-line reproducer:\n\n```php\nfunction f(): mixed { return [1, 2]; }\necho implode(\",\", (array) f());   // Segmentation fault (exit 139)\n```\n\nROOT CAUSE, isolated by measurement rather than reading:\n- `implode_runtime_label` (`src/codegen/lower_inst/builtins/strings/split.rs:567`) maps `PhpType::Mixed` to the generic `__rt_implode`. It cannot do better — the element type is unknown statically.\n- `__rt_implode` (`src/codegen_support/runtime/strings/implode.rs`) dispatches on the array's own `value_type` tag, read from the packed metadata word at `[array_ptr - 8]`. It handles exactly TWO layouts: tag 7 (boxed Mixed cells, via `__rt_mixed_cast_string`) and everything else as `(ptr, len)` STRING PAIRS.\n- An array of raw ints is neither. Each int is read as a string pointer and dereferenced.\n\nThe discriminating sweep — same shape, only the element type varying:\n| mixed holds | result |\n|---|---|\n| `[\"a\",\"b\"]` (string elements) | IDENTICAL to php |\n| `[1, 2]` (int elements) | SEGFAULT |\n| `[\"x\"=>1,\"y\"=>2]` (assoc, int values) | SEGFAULT |\n| `[]` (empty) | IDENTICAL to php |\n\nSo it is the ELEMENT TYPE, not indexed-vs-hash. `count()`, `array_keys()` and `var_dump()` on the identical value all behave correctly, which is what rules out the cast's representation and points at this one helper.\n\nSUGGESTED FIX SHAPE: complete the existing tag switch in `__rt_implode` — add the int arm (what `__rt_implode_int` already does through `__rt_itoa`) and the bool arm (`\"1\"`/`\"\"`, NOT `\"1\"`/`\"0\"`, per `implode_element_runtime_label`'s comment). Doing it in the helper rather than at the lowering keeps one copy of the tag logic. ⚠️ The helper's docblock pins TWO invariants in the boxed-Mixed path — the owned mixed-cast slot (ownership, #601) and the live `_concat_off` cursor — and both must survive a rework; they are pinned by `tests/codegen/runtime_gc/regressions.rs` and `tests/array_result_type_tests.rs`.\n\nAlso found while probing: `(array)` cast of a value already statically typed `array` is REFUSED — \"unsupported EIR backend feature: cast to EIR type Heap(Array)\". And `array_sum()` on a `Mixed`-typed array is refused by the checker (\"argument must be array\").\nEvidence: Five probes compiled and run against `php -n` 8.5.6, one file per case so a crash cannot mask the rest. Emitted assembly confirms the call site unboxes the Mixed and passes the payload in x3 to `__rt_implode`, so the renderer choice — not the argument — is what is wrong.\nVerified by: Reproduced on macOS arm64 with the branch compiler; exit code 139 on the int-element shapes, byte-identical output on the string-element and empty shapes.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"superseded","confidence":0.7,"tags":["session-learning","implode","mixed","segfault","runtime-helper","open-bug"],"paths":["src/codegen_support/runtime/strings/implode.rs","src/codegen/lower_inst/builtins/strings/split.rs"],"stack":["rust","php","aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T17:04:11.160Z"}],"context":{"fact":"`implode()` SEGFAULTS on an array whose static type is `Mixed` when the runtime elements are ints. Four-line reproducer:","verification":"Five probes compiled and run against `php -n` 8.5.6, one file per case so a crash cannot mask the rest. Emitted assembly confirms the call site unboxes the Mixed and passes the payload in x3 to `__rt_implode`, so the renderer choice — not the argument — is what is wrong."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T17:04:11.160Z","path_fingerprints":[{"path":"src/codegen_support/runtime/strings/implode.rs","sha256":"076d31975775f41f0992f97119c8902884f218a77351ac87f2ef940118d09ef0","size":34353},{"path":"src/codegen/lower_inst/builtins/strings/split.rs","sha256":"a3b6d648056d01e0ad471c3408eef691b5a1797f6cca1d795ad3f3ac1986bfe5","size":33538}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture","superseded_at":"2026-09-12T16:40:42.881Z","superseded_by":"repo:sparkling-jingling-flute:bug_fix:implode-s-segfault-on-int-elements-is-fixed-the-layout-dispatch-now-covers-all-f","superseded_reason":"The old packet is tagged open-bug and describes the segfault as live, with a SUGGESTED FIX SHAPE. Commit 44baea7183 fixed it, and went wider than the suggestion (float was missing from the suggested arms and was equally broken). Leaving it recallable would send a future session to re-diagnose a closed bug from an incomplete fix plan."},"edges":[{"relation":"superseded_by","to":"repo:sparkling-jingling-flute:bug_fix:implode-s-segfault-on-int-elements-is-fixed-the-layout-dispatch-now-covers-all-f","evidence":"The old packet is tagged open-bug and describes the segfault as live, with a SUGGESTED FIX SHAPE. Commit 44baea7183 fixed it, and went wider than the suggestion (float was missing from the suggested arms and was equally broken). Leaving it recallable would send a future session to re-diagnose a closed bug from an incomplete fix plan.","created_at":"2026-09-12T16:40:42.881Z"}],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":681,"superseded_by":"repo:sparkling-jingling-flute:bug_fix:implode-s-segfault-on-int-elements-is-fixed-the-layout-dispatch-now-covers-all-f","superseded_reason":"The old packet is tagged open-bug and describes the segfault as live, with a SUGGESTED FIX SHAPE. Commit 44baea7183 fixed it, and went wider than the suggestion (float was missing from the suggested arms and was equally broken). Leaving it recallable would send a future session to re-diagnose a closed bug from an incomplete fix plan."},"created_at":"2026-09-11T17:04:11.160Z","updated_at":"2026-09-12T16:40:42.881Z","author_branch":"feat/opcache-runtime-cache"}
```

