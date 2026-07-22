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
does not. Bridge archives are discovered and linked by `src/linker.rs` rather
than emitted by the assembly backend.

Runtime feature selection is derived from the EIR module plus CLI-owned modes
such as `--web`. This keeps ordinary binaries from carrying unused helper
families while preserving deterministic linking.

## Builtin Boundary

Each registry-backed builtin owns one backend-neutral `BuiltinSemantics`
descriptor in `src/builtins/<area>/<name>.rs`. Checker validation/result typing,
optimizer effects, ownership/aliasing, runtime/link requirements, callable policy,
argument lowering, and EIR lowering consume that same descriptor.

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
target-aware call helpers on macOS ARM64, Linux ARM64, and Linux x86_64. See
[Eval Runtime Architecture](eval-runtime.md) for the complete boundary.

## Emit Modes

`--emit executable` is the default and emits a process entry point. `--emit
cdylib` emits a PIC user object with `#[Export]` trampolines and lifecycle
symbols for embedding hosts. On Linux cdylib output also hides internal runtime
symbols so separate loaded elephc modules do not preempt each other's state.

## Key Mechanisms

### Mixed boxing

`src/codegen_support/value_boxing.rs` owns the shared emitters that box PHP
values into runtime `Mixed` cells. A boxed cell pairs a runtime tag byte
(0 int, 1 str, 2 float, 3 bool/false, 4 array, 5 assoc array, 6 object,
7 mixed/union/iterable, 8 null) with the payload; tag values and payload
register conventions must match `__rt_mixed_from_value`. Owned boxing paths
transfer or release references so payloads are never double-freed. `Union(...)`
and `Iterable` values reuse the same boxed representation.

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

## Backend Contract

- PHP-visible behavior belongs in `src/ir_lower/` and `src/codegen/`.
- Shared runtime, ABI, platform, and metadata helpers belong in
  `src/codegen_support/`.
