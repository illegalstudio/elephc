---
title: "The Code Generator"
description: "How EIR becomes target assembly and links against the shared runtime."
sidebar:
  order: 7
---

**Source:** assembly emitter `src/codegen/`; EIR lowering `src/ir_lower/`;
literal eval planning `src/eval_aot.rs`; IR model and validation `src/ir/`;
shared runtime/ABI support `src/codegen_support/`; optional bridge crates under
`crates/`.

Codegen is a single EIR pipeline. The checked and optimized AST is always
lowered into EIR, IR passes run over that module, and `src/codegen/` emits the
user assembly for the selected target.

## Pipeline Position

```text
PHP source
  -> Lexer
  -> Parser
  -> Magic constants
  -> Conditional compilation
  -> Resolver / autoload
  -> NameResolver
  -> AST constant folding
  -> Type checker / warnings
  -> AST optimizer passes
  -> AST -> EIR lowering
  -> EIR validation
  -> EIR optimization passes
  -> EIR -> target assembly
  -> runtime cache
  -> assembler / linker
  -> binary or cdylib
```

`--emit-ir` stops after lowering and IR optimization, printing the textual EIR.
Normal builds continue through `codegen::generate_user_asm_from_ir_with_options`
and link the resulting user object against the cached runtime object.

## Module Layout

| Path | Responsibility |
|---|---|
| `src/codegen/mod.rs` | Public codegen facade, EIR backend entry points, runtime metadata finalization |
| `src/codegen/block_emit.rs` | Function/block traversal, prologues, top-level entry and deferred EIR wrappers |
| `src/codegen/lower_inst.rs`, `src/codegen/lower_inst/` | Instruction lowering, including typed runtime-target dispatch with no PHP-name lookup |
| `src/codegen/lower_inst/runtime_calls.rs`, `runtime_functions/` | Validates and lowers typed `RuntimeCallTarget` / `RuntimeFnId` operations into target-aware backend implementations |
| `src/codegen/lower_inst/builtins/eval.rs` | Literal-eval AOT bodies, scope synchronization, bridge calls, and dynamic post-barrier dispatch |
| `src/codegen/lower_term.rs` | Terminator lowering for returns, branches, switches, and unreachable paths |
| `src/codegen/context.rs` | EIR function emission state and value materialization helpers |
| `src/codegen/frame.rs` | Stack-frame sizing, local slots, register allocation integration |
| `src/codegen/value_placement.rs` | Stack/register placement for EIR values |
| `src/codegen/runtime_callable_invoker.rs` | Generated uniform `(descriptor, argument array) -> Mixed` invoker trampolines |
| `src/codegen_support/abi/` | Target ABI helpers for registers, calls, stack slots, symbols, and frame mechanics |
| `src/codegen_support/arrays.rs` | Shared array metadata helpers used by EIR and runtime support |
| `src/codegen_support/callable_descriptor.rs` | Callable descriptor layout, kind constants, and entry-slot loading |
| `src/codegen_support/callable_invoker_args.rs` | Shared descriptor-invoker argument cloning and boxing helpers |
| `src/codegen_support/sentinels.rs` | Null/uninitialized sentinel constants, tagged-scalar helpers, x86_64 heap-header magic |
| `src/codegen_support/value_boxing.rs` | Shared scalar/string/array/object/iterable boxing into runtime `Mixed` cells |
| `src/codegen_support/wrappers/` | Shared callback and fiber wrapper emitters used by deferred EIR wrapper emission |
| `src/codegen_support/runtime/` | Shared `__rt_*` routines and runtime data emission |
| `src/codegen_support/runtime/eval_bridge.rs`, `eval_scope.rs` | C-ABI value hooks and core materialized-scope helpers for eval |
| `src/codegen_support/platform/` | Target descriptions and assembler/linker naming conventions |

The active backend must remain target-aware. New lowering paths should use the
ABI helpers instead of hardcoding AArch64 or x86_64 register and stack details.

## Runtime Split

Codegen always produces two compiler-owned artifacts:

1. **User assembly** from `src/codegen/`, containing lowered PHP functions,
   methods, top-level entry code, user metadata, and literal data.
2. **Runtime object** from `src/codegen_support/runtime/`, cached by compiler
   version, target, heap size, runtime features, and PIC mode.

The linker may also add optional bridge staticlibs as a third class of input.
For example, dynamic eval links `libelephc_magician.a`; fully AOT literal eval
does not. Bridge archives are discovered and linked by `src/linker/` rather
than emitted by the assembly backend.

Runtime feature selection is derived from the EIR module plus CLI-owned modes
such as `--web`. This keeps ordinary binaries from carrying unused helper
families while preserving deterministic linking.

## Builtin Boundary

Each PHP builtin surface has one dependency-neutral contract in
`crates/elephc-builtin-contract`. The AOT home file in
`src/builtins/<area>/<name>.rs` joins its `BuiltinSemantics` by stable `BuiltinId`.
Checker validation/result typing, optimizer effects, ownership/aliasing,
runtime/link requirements, callable policy, argument lowering, and EIR lowering
consume that assembled view without repeating signature metadata.

Lowering emits reusable EIR primitives/graphs or an `Op::RuntimeCall` carrying a
typed `RuntimeCallTarget`. `src/codegen/lower_inst/runtime_calls.rs` and the bounded
`runtime_functions/` groups select the concrete target-aware implementation. PHP
builtin names are absent from backend dispatch. Only compiler-resident language
constructs such as `eval`, `isset`, `unset`, `empty`, `exit`, and `die` retain the
separate `LanguageConstructCall` path.

## Eval Lowering Boundary

Literal `eval()` calls reach EIR as `EvalLiteralCall`. The shared planner in
`src/eval_aot.rs` classifies the fragment before target assembly is chosen:

1. Direct-local and fully static plans become native instructions or internal
   EIR functions and require no eval runtime state.
2. Scope-backed AOT plans use `EvalScopeGet`/`EvalScopeSet` and enable only the
   core `eval_scope` runtime feature.
3. Dynamic or unsupported plans materialize `EvalContext`, `EvalScope`, and
   optional `EvalGlobalScope` slots, call the Magician C ABI, and enable
   `eval_bridge`.

The lowerer preserves PHP source evaluation order before ABI materialization,
boxes values into the shared `Mixed` cell representation, and uses the normal
target-aware call helpers on the three executable hosts: macOS ARM64, Linux
ARM64, and Linux x86_64. See [Eval Runtime Architecture](eval-runtime.md) for the
complete boundary.

## Emit Modes

`--emit executable` is the default and emits a process entry point. `--emit
cdylib` emits a PIC user object with `#[Export]` trampolines, ABI/error/memory
helpers, and lifecycle symbols for embedding hosts. Every export uses a native
exception boundary; scalar exports preserve their C signatures and publish
status through `elephc_last_status()`, while string-return exports preserve
their fixed scalar/string inputs, append output pointer/length parameters,
return status directly, and copy the PHP byte string into caller-owned heap
storage. Boundary nesting is explicit and concat scratch state is saved and
restored around each entry. Internal symbols are hidden on ELF (including
driver-supplied `_init`/`_fini`) and private externs on Mach-O, so separate
loaded Elephc modules do not preempt each other's runtime state and Mach-O
`-dead_strip` has real roots to collect against.

## Key Mechanisms

### Mixed boxing

`src/codegen_support/value_boxing.rs` owns the shared emitters that box PHP
values into runtime `Mixed` cells. A boxed cell pairs a runtime tag byte
(0 int, 1 str, 2 float, 3 bool/false, 4 array, 5 assoc array, 6 object,
7 mixed/union/iterable, 8 null) with the payload; tag values and payload
register conventions must match `__rt_mixed_from_value`. Owned boxing paths
transfer or release references so payloads are never double-freed. `Union(...)`
and `Iterable` values reuse the same boxed representation.

### Staging a builtin's integer arguments

A builtin whose integer argument may arrive BOXED cannot load its arguments in
ABI order. Unboxing calls `__rt_mixed_cast_int`, which clobbers every
caller-saved argument register, so any argument already loaded would be
destroyed by the unboxing of a later one.

The rule is: resolve every boxed integer argument FIRST, park each on the stack,
load the remaining arguments into their ABI registers, then pop the parked ones
back — in reverse staging order, since the stack is LIFO. An argument that is not
boxed is loaded in place, which keeps the common path identical to a builtin that
never needed staging at all.

`array_fill()` is the worked example
(`src/codegen/lower_inst/builtins/arrays/fill_helpers.rs`): `$start` and `$count`
both go through `stage_boxed_fill_integer` / `settle_fill_integer`, and the
string fill helper's `(count, ptr, len)` ABI — which puts the count in the FIRST
argument register rather than the second — is settled the same way.

Which representations need staging:

- `Int` / `Bool` — no. One register, loaded directly.
- `TaggedScalar` — no. The payload lives in the value's own slot and the null tag
  in the adjacent one, so loading the slot into an ABI register already yields the
  integer. A `?int` that is genuinely `null` still reaches the builtin as its
  sentinel payload rather than PHP's "passing null is deprecated, treated as 0",
  which is a gap in the tagged-null surface and not in the staging.
- `Mixed` / `Union` — yes. `resolve_int_operand_to_result` unboxes them through
  `__rt_mixed_cast_int`, which is the call that forces the ordering.

A local whose value came from checked integer arithmetic is `Mixed`, because the
product or sum may overflow to float: `$n = $this->w * $this->h` is boxed while a
literal, a parameter, or the same expression folded at compile time is not. That
asymmetry is why a missing stage looks like a defect that only happens inside a
method (issue #502).

### Callable descriptors and invokers

`PhpType::Callable` stays one pointer wide, but the pointer targets a
descriptor (`src/codegen_support/callable_descriptor.rs`) whose entry slot is
loaded before invoking native code. Descriptors record the callable kind
(closure, first-class callable, callback adapter, object invoke, plain
function, builtin, extern, static method, instance method) plus
signature/default/by-ref/variadic metadata and capture/receiver environment,
without changing the one-word callable ABI. The optional invoker slot points at
a generated uniform adapter (`src/codegen/runtime_callable_invoker.rs`) with
the ABI `(descriptor, argument array) -> Mixed`. The invoker saves and restores
the caller's callee-saved registers it scratches — `x19`-`x26` on AArch64 and
`r12`, `rbx`, `r13`-`r15` on x86_64 — in a dedicated frame save area.

### Static and global storage

Function `static` locals are `.comm` symbols (`_static_<fn>_<name>`, 16 bytes)
paired with a one-time init-marker symbol (`<symbol>_init`); the `--web` reset
generator walks these records to release and re-arm persistent statics between
requests. Static properties live behind per-class user-data symbols
(`crate::names::static_property_symbol`), with late-bound `static::` receivers
resolved through native class-id branches. `global` variables load and store
through `_eir_global_<mangled_fqn>` symbols emitted for the EIR `LoadGlobal` /
`StoreGlobal` instructions.

### Call-stack overflow guard

`src/codegen/stack_guard.rs` emits a stack-depth check into every compiled
function, method, closure, and generator body prologue, immediately after the
frame has been reserved (so the compare already accounts for that frame) and
before the incoming parameters are spilled. It is an unsigned comparison of the
stack pointer against the runtime `_stack_limit` word, branching to
`__rt_stack_overflow` when the pointer is below it:

- **AArch64** (macOS and Linux): `adrp` / `ldr` / `cmp sp, x9` / `b.hs <local>` /
  `b __rt_stack_overflow` — five instructions, clobbering only `x9`. The
  conditional branch stays local because `b.cond` only encodes a ±1 MiB
  displacement, which large programs exceed; the unconditional `b` reaches
  ±128 MiB and gets linker veneers beyond that.
- **x86_64**: `cmp rsp, QWORD PTR [rip + _stack_limit]` / `jb __rt_stack_overflow`
  — two instructions, clobbering nothing.

The check never writes memory, so it is safe to run when the remaining stack is a
single page. A zero `_stack_limit` makes it always pass, which is the state
before `__rt_stack_limit_init` runs and whenever the stack bounds could not be
determined — the guard is inert rather than wrong. `main` is not guarded: it is
the root of every process-entry call chain and runs before the floor exists.
Cdylibs publish the floor from `elephc_init()` because they have no process
entry point. See [The runtime](the-runtime.md) for the floor measurement and
the fiber handoff.

### Sentinels and null representation

`src/codegen_support/sentinels.rs` is the canonical home for the in-band
sentinel constants and tagged-scalar helpers. Under the default
`NullRepr::Tagged` mode, null-capable scalar slots use the inline two-word
`{payload, tag}` `TaggedScalar` representation, making the full i64 range
representable; the legacy `--null-repr=sentinel` opt-out stores the in-band
`NULL_SENTINEL` (`PHP_INT_MAX - 1`), which collides with that real integer.
Uninitialized typed properties use a separate sentinel (`PHP_INT_MAX - 2`)
stored in the property's metadata word, never the value word, so it cannot
collide with property values. On x86_64, heap headers carry the `ELPH` magic
in the high 32 bits of the kind word: every stamp goes through the shared
`x86_64_heap_kind_word` helper and every check compares against
`X86_64_HEAP_MAGIC_HI32` — local copies of either constant are forbidden.

### Nullable optional string lengths

`substr()`, `substr_replace()`, and `substr_count()` share one PHP contract for
their optional `$length`: omission and `null` mean “through the end”, while `0`
means an empty window (or pure insertion for `substr_replace()`) and a negative
integer keeps its PHP end-relative meaning. The backend must therefore inspect
null before applying the ordinary weak integer conversion, because PHP's
general `(int) null` result of `0` is not the argument contract for these
builtins.

Named-argument planning can materialize an omitted nullable default as a null
EIR operand, so operand count alone is not authoritative. String builtin
lowering classifies the operand as absent/static null, concrete, inline
`TaggedScalar`, or boxed `Mixed`/union. Runtime-null tagged and boxed values are
mapped to the through-end path; concrete zero and negative payloads continue
through numeric length normalization unchanged.

The three builtins use different final encodings:

- `substr()` is lowered inline and maps runtime null to a saturating positive
  length before clamping it to the remaining string.
- `substr_replace()` passes `i64::MAX` to `__rt_substr_replace` for omitted or
  null length. `-1` is never an omission sentinel because it is a real PHP
  negative length.
- `substr_count()` cannot use the saturating value because its explicit-length
  bounds check would raise `ValueError`. It preserves null status separately
  and selects `subject_length - resolved_offset` before validating concrete
  lengths.

For boxed values, the weak-integer helper keeps the original `Mixed` cell
pointer in an aligned temporary stack slot across `__rt_mixed_unbox` and
`__rt_mixed_cast_int`, preserving existing float-coercion diagnostics. The
normalized payload and null/int tag use `x0`/`x1` on AArch64 and `rax`/`rdx` on
x86_64. `substr_count()` copies the tag to `x7` or `r11` only after conversion
calls have finished, then restores the subject, needle, and offset ABI
registers before window validation.

## Backend Contract

- PHP-visible behavior belongs in `src/ir_lower/` and `src/codegen/`.
- Shared runtime, ABI, platform, and metadata helpers belong in
  `src/codegen_support/`.
