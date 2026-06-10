# Null-sentinel collision — Implementation Plan

**Goal:** Make every valid PHP integer representable as a non-null scalar — eliminating the collision where the integer `9223372036854775806` (= `PHP_INT_MAX - 1` = the in-band null sentinel `0x7fff_ffff_ffff_fffe`) is misread as `null` — without heap-boxing every plain `int`.

**Architecture:** elephc stores PHP `null` in an unboxed scalar slot using the magic i64 `0x7fff_ffff_ffff_fffe`. Because every i64 bit pattern is a valid PHP int, this in-band sentinel collides with the real integer. The fix introduces an inline 2-word `{tag, payload}` tagged scalar for scalar slots that can be null, reusing the existing runtime tag scheme, with no heap allocation. Designed to converge with the int-overflow→float work (same `{tag, payload}` shape, tag ∈ {int, float, null}).

## Phases

- **Phase 0** — Lock repros (`tests/codegen/null_sentinel/repros.rs`), decide representation (see DESIGN-NOTES), add microbench harness.
- **Phase 1** — Unify the 7 file-local `NULL_SENTINEL` consts + raw literals into one canonical constant; document the incompatibility in `docs/php/types.md`. Zero behavior change.
- **Phase 2** — Tagged representation behind `NullRepr::{Sentinel, Tagged}` flag (default `Sentinel`); audit every producer/consumer; repros pass under `Tagged` on all 3 targets.
- **Phase 3** — Narrowing: non-nullable `int` slots stay plain i64 (asm-inspection proof); only genuinely-nullable slots widen.
- **Phase 4** — Flip default to `Tagged`, update docs, converge with the overflow plan.

## Sentinel inventory

**Producers (write the null sentinel):** `expr/scalars.rs` (`emit_null_literal`), `expr/objects/nullsafe.rs` (`?->`), `expr/objects/dispatch/enums.rs` (enum `tryFrom`), `expr/objects.rs` (`emit_boxed_null`), `expr/arrays/access/indexed.rs` (array miss), `abi/symbols.rs` (globals), `stmt/assignments/properties/storage.rs` (Void property), `abi/values.rs` (Void/Never local store), `runtime/arrays/array_shift.rs`, boxed-null low-word reuses in `magic_set.rs`, `dynamic_props.rs`.

**Consumers (detect null via sentinel):** `builtins/types/is_null.rs`, `stmt/io.rs` (echo), `builtins/io/var_dump.rs`, `functions/generator/emit/stmts.rs`, `expr/compare/null_coalesce.rs` (`??`), `stmt/null_coalesce_assign.rs` + dup in `stmt/assignments/locals.rs` (`??=`), `builtins/arrays/isset.rs`, `expr/coerce.rs` (`coerce_null_to_zero`), `builtins/io/stream_get_contents.rs`.

---

## DESIGN-NOTES (Phase 0)

### Empirical findings (macOS ARM64, elephc 0.23.8, PHP 8.4.20 oracle)

```
echo 9223372036854775806;                  elephc: (empty)        PHP: 9223372036854775806
$a=[...806]; echo $a[0]; var_dump($a[0]);  elephc: (empty)/NULL   PHP: ...806 / int(...806)
function f():?int{return ...806;} f()      elephc: NULL           PHP: int(...806)
$x=...806; var_dump(is_null($x));          elephc: bool(true)     PHP: bool(false)
$x=...806; echo $x ?? 0;                   elephc: 0              PHP: ...806
$a=[...806]; isset($a[0])                  elephc: true (correct)
```

Key discovery from asm inspection (`--emit-asm`): a `?int` return is **already boxed as a Mixed cell with the correct int tag** (`__rt_mixed_from_value(tag=0, payload=...806)`); the NULL output comes from the **shared int formatter** (`emit_var_dump_int`) re-checking the payload against the sentinel after unboxing. The collision therefore lives in (a) consumers that sentinel-check every `Int`-typed payload because the type `Int` is overloaded ("definitely int" vs "int-or-null"), and (b) producers that write the sentinel into `Int`-typed slots (array miss, etc.). The boxed Mixed path itself is sound.

Unrelated pre-existing quirk found while locking repros: `var_dump(isset(...))` prints `int(1)` instead of `bool(true)` (isset result is typed int). Repro tests avoid it via ternary echo.

### Decision: option (b) — inline 2-word tagged scalar, new internal `PhpType::TaggedScalar` variant

1. **Carrier.** New variant `PhpType::TaggedScalar` (internal-only, like `Mixed`; the checker never produces it). It is **constructed only in codegen funnels and only when `NullRepr::Tagged` is active**. Under `NullRepr::Sentinel` (default) it is never constructed, so generated code is byte-identical and no flag plumbing is needed in `src/types/model.rs` — `stack_size`/`register_count`/`is_refcounted` for the variant are unconditional.

2. **Layout.**
   - Stack slot: 16 bytes — payload word at `offset`, tag word at `offset - 8` (mirrors `Str`: ptr at `offset`, len at `offset - 8`).
   - `stack_size() = 16`, `register_count() = 2`, `is_refcounted() = false`, `is_float_reg() = false`.
   - Expression result: payload in `int_result_reg` (ARM64 `x0`, x86_64 `rax`), tag in the second-word register (ARM64 `x1`, x86_64 `rdx` — same convention as `Str`'s second word). Payload-in-result-reg makes "narrow to plain int" free once non-null is proven.
   - Arguments: 2 consecutive integer argument registers (payload, tag), reusing the existing 2-register argument machinery built for `Str`; stack spills follow the existing 2-word logic.

3. **Tag domain.** Reuse `runtime_value_tag` values: `0 = int`, `2 = float` (reserved for the int-overflow plan), `3 = bool` (reserved), `8 = null`. `is_null` ⇔ `tag == 8`. The `{tag, payload}` pair is word-compatible with a Mixed cell's words 0/8, so TaggedScalar→Mixed boxing is a direct `__rt_mixed_from_value(tag, payload, 0)` and Mixed→TaggedScalar unboxing is two loads. This is the convergence point with the overflow plan.

4. **Construction funnels (under `Tagged`).**
   - Declared-type lowering (`codegen_static_type`/`codegen_declared_type` + the `Union → Mixed` collapse): `Union([Int, Void])` → `TaggedScalar` for params, returns, locals, properties.
   - `array<int>` element reads (miss-capable): hit → `{tag=int, payload}`, miss → `{tag=null}`.
   - `null` literal flowing into an int context; `?->` over int members; `array_shift` empty result; `stream_get_contents` failure path.
   - **Scope:** only `int|null` widens initially. `?float`/`?bool` keep today's Mixed boxing (tag values reserved; flipping them is follow-up work). `?string` stays Mixed permanently (a string is already 2 words; `{tag, ptr, len}` would need 3). Nullable objects keep their existing pointer-sentinel scheme (separate surface, out of scope).

5. **Consumers (under `Tagged`).** `is_null`, echo suppression, `var_dump`, `??`, `??=`, `isset`, `coerce_null_to_zero` dispatch on the tag word when the operand is `TaggedScalar`, and emit **no sentinel check at all** for plain `Int` operands (plain `Int` becomes "definitely int" again — this alone fixes `echo 9223372036854775806`).

6. **Flag.** `NullRepr::{Sentinel, Tagged}` lives in `CliConfig` → pipeline → codegen `Context`. Default `Sentinel` until Phase 4. CLI/env opt-in: `ELEPHC_NULL_REPR=tagged`. Tests force `Tagged` per-compile through an explicit parameter on the test-support compile helpers (no process-global mutation — tests run in parallel threads).

7. **Rejected alternatives.**
   - *(a) rarer sentinel value:* still collides — every i64 is a valid int.
   - *(c) box null-capable scalars as Mixed:* already today's repr for `?int` returns yet still broken (consumer-side sentinel re-check), and boxing every `array<int>` read would heap-allocate per access.
   - *Re-repr `Union([Int,Void])` without a new variant:* every existing `Union(_) => /* boxed pointer */` match arm becomes a silent hazard under the flag; a new variant turns missed paths into compile errors (exhaustive matches) or loud crashes instead of silent miscompilation.
   - *Uninit-property sentinel (`0x...fffd`):* not a value collision (separate metadata word); in scope only for the Phase 1 constant unification.
