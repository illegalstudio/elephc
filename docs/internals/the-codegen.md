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
| `src/codegen/lower_inst.rs`, `src/codegen/lower_inst/` | Instruction lowering and builtin-specific EIR emission |
| `src/codegen/lower_inst/builtins/eval.rs` | Literal-eval AOT bodies, scope synchronization, bridge calls, and dynamic post-barrier dispatch |
| `src/codegen/lower_term.rs` | Terminator lowering for returns, branches, switches, and unreachable paths |
| `src/codegen/context.rs` | EIR function emission state and value materialization helpers |
| `src/codegen/frame.rs` | Stack-frame sizing, local slots, register allocation integration |
| `src/codegen/value_placement.rs` | Stack/register placement for EIR values |
| `src/codegen_support/abi/` | Target ABI helpers for registers, calls, stack slots, symbols, and frame mechanics |
| `src/codegen_support/arrays.rs` | Shared array metadata helpers used by EIR and runtime support |
| `src/codegen_support/callable_invoker_args.rs` | Shared descriptor-invoker argument cloning and boxing helpers |
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

## Backend Contract

- PHP-visible behavior belongs in `src/ir_lower/` and `src/codegen/`.
- Shared runtime, ABI, platform, and metadata helpers belong in
  `src/codegen_support/`.
