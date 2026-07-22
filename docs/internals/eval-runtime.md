---
title: "Eval Runtime Architecture"
description: "How literal eval is planned for AOT lowering, when Magician is linked, and how native and interpreted scopes share values."
sidebar:
  order: 9
---

**Sources:** `src/eval_aot.rs`, `src/ir_lower/expr/mod.rs`,
`src/ir_lower/program.rs`, `src/codegen/lower_inst/builtins/eval.rs`,
`src/codegen_support/runtime/eval_bridge.rs`,
`src/codegen_support/runtime/eval_scope.rs`, and
`crates/elephc-magician/`.

Eval is an experimental hybrid feature. Ordinary elephc source and eligible
literal eval fragments are compiled ahead of time. Only fragments that require
runtime parsing or dynamic symbol behavior use the optional Magician
interpreter. The resulting executable remains standalone: Magician is a static
bridge library, not an external PHP or Zend dependency.

For PHP-visible behavior and the supported fragment subset, see
[Eval](../php/eval.md). This page documents the compiler/runtime boundary.

## Pipeline position

`eval` is not a dedicated lexer token or AST node. The parser produces an
ordinary `ExprKind::FunctionCall`; later semantic passes recognize the
case-insensitive PHP language-construct name.

```text
eval($code)
  -> checker: exactly one argument, result Mixed, conservative barrier
  -> EIR lowering: LanguageConstructCall or EvalLiteralCall
  -> literal planner, when the source is statically known
       -> internal no-scope EIR function
       -> internal EIR function with direct read parameters
       -> internal scope-aware EIR function plus core eval-scope helpers
       -> Magician interpreter fallback
  -> target-aware assembly and optional bridge linking
```

The checker and AST optimizer deliberately stay conservative even for a source
literal. The more precise AOT decision happens during EIR lowering, after the
front end has already preserved PHP's dynamic semantics and diagnostics.

## Execution-path decision

| Path | Typical input | Runtime requirements |
|---|---|---|
| No-scope AOT | A literal fragment with no caller-scope access | Internal EIR function; no eval context, scope, or Magician library |
| Direct-read AOT | A statically lowerable literal with read-only caller values | Internal EIR function with boxed `Mixed` parameters; no eval scope or Magician library |
| Scope-backed AOT | A statically lowerable literal with known scope writes | Internal EIR function plus core `eval_scope`; no interpreter library |
| Interpreter fallback | A dynamic string or a literal requiring dynamic declarations, includes, references, dynamic calls, or another unsupported AOT shape | `eval_bridge`, synchronized scopes, PCRE2, and `elephc_magician` |

`src/eval_aot.rs` parses literal fragments at compile time, applies call-site
magic-constant metadata, records known scope reads and writes, and produces an
`EvalAotPlan`. A plan can contain a no-scope EIR body, a scope-aware EIR body,
or a conservative fallback reason. `src/ir_lower/program.rs` materializes each
accepted body as a deterministic `__eir@evalaot*` function before validation,
optimization, register allocation, and normal target-aware codegen.

Current fallback classes include parse failures, `include`/`require`, runtime
declarations, global/static scope, references and by-reference operations,
dynamic calls or class/member resolution, unsupported object/array/iterable
shapes, `try`/`throw`, unsupported control flow, and unsupported static calls.
Eligibility is intentionally conservative: a fragment falls back rather than
being partially compiled with different observable behavior.

`src/ir_lower/program.rs` repeats the final bridge-requirement check against the
completed EIR module. This accounts for actual local-slot types and supported
static function/method targets before setting `RuntimeFeatures::eval_bridge`.
Consequently, the presence of `EvalLiteralCall` alone does not imply that the
binary links Magician.

## EIR representation

Literal calls use `EvalLiteralCall`, carrying the fragment in the module data
pool. Dynamic calls remain compiler-resident `LanguageConstructCall` operations
until the eval lowering path materializes the runtime code string. Registry-backed
builtins use typed `RuntimeCall` targets instead and never participate in eval-name
dispatch.

The eval-specific EIR operations are:

| Operation | Responsibility |
|---|---|
| `EvalLiteralCall` | Preserve a literal fragment for AOT planning or interpreter fallback. |
| `EvalScopeGet`, `EvalScopeSet` | Read or update a named boxed cell in a materialized eval scope. |
| `EvalFunctionCall`, `EvalFunctionCallArray` | Dispatch a function created or registered in the persistent eval context. |
| `EvalFunctionExists`, `EvalClassExists` | Probe dynamic symbols that may have been created by an earlier eval barrier. |
| `EvalConstantExists`, `EvalConstantFetch` | Probe or fetch constants retained in the eval context. |
| `EvalObjectNew` | Construct a class that may have been declared at runtime. |
| `EvalStaticMethodCall` | Dispatch a static method whose target may come from eval metadata. |

The conservative default effects for eval calls include arbitrary observable
call effects. Symbol probes read global state; scope access reads or writes heap
state and can fail; constant fetches also carry ownership/refcount effects.
Later lowering may refine a literal call after the AOT plan proves a narrower
path.

Three addressable local kinds hold eval state when required:

| `LocalKind` | Lifetime and role |
|---|---|
| `EvalContext` | Persistent Magician context for declarations, constants, callable metadata, and interpreter state. Its presence requires the full bridge. |
| `EvalScope` | Materialized activation/closure scope shared with the executing fragment. |
| `EvalGlobalScope` | Materialized program-global scope used by `global` aliases and CLI argument globals. |

Frame sizing and cleanup see these slots before assembly emission. A no-scope
or direct-read AOT fragment does not declare them merely because the source
contains `eval()`.

## Checker and optimizer barrier

The type checker enforces exactly one argument, infers that argument for its
side effects, and gives the call the static type `Mixed`. After the call it:

- marks the active statement stream as having crossed eval;
- widens known local types to `Mixed`;
- drops closure, callable-signature, capture, and callable-target facts;
- permits later reads of variables and dynamic symbols that eval may have
  created.

AST constant propagation reports `Invalidation::All` for eval. This prevents a
pre-call constant or alias fact from being reused after code that can create,
overwrite, or unset caller-visible state. EIR planning may later omit the
physical runtime barrier for a proven literal path without weakening those
front-end safety rules.

## Dynamic bridge lifecycle

When interpreter fallback is required, generated code performs these steps:

1. Coerce the code argument to a PHP string.
2. Lazily allocate the `EvalContext`, activation `EvalScope`, and, when needed,
   `EvalGlobalScope`.
3. Register bridge-compatible AOT functions, methods, constructors, class
   metadata, parameter names, defaults, and visibility information.
4. Flush visible locals, by-reference cells, closure captures, and eligible
   globals into boxed scope cells.
5. Set call-site file, directory, namespace, class, trait, function, and method
   metadata used by magic constants.
6. Call `__elephc_eval_execute` through the target-aware ABI.
7. Reload dirty, created, or unset scope entries and propagate return/fatal/
   throwable state through the normal generated runtime paths.

Top-level scope setup also seeds `$argc` and `$argv`. Function fragments can
bind those values or compiler-known program globals with PHP `global` aliases.
By-value closure captures synchronize only their captured copy; by-reference
captures share a ref cell whose storage is widened before it crosses the
barrier when necessary.

## Shared value ABI and ownership

Magician does not introduce a second PHP value layout. The generated runtime
exports C-ABI hooks that box, unbox, retain, release, compare, cast, iterate,
and mutate the same `Mixed` cells used by native code. Array writes still pass
through the normal copy-on-write helpers, and object/class operations reuse
generated metadata when the bridge shape is supported.

Scope setters retain the value stored in the context; getters return values
with the ownership expected by their EIR result. Normal returns, runtime
fatals, thrown values, early fragment returns, and function cleanup must all
balance those cells. Persistent declarations and metadata live in the eval
context until its owning generated function or process scope is destroyed.

## Parsing and cache

Dynamic source is parsed into Magician's immutable EvalIR. The process-wide
parse cache stores both successful parse results and parse errors by exact
source bytes:

- FIFO capacity: 256 distinct fragments;
- maximum cacheable fragment size: 64 KiB;
- cached data: immutable `Arc<EvalProgram>` or `EvalParseError` only;
- excluded data: scopes, cells, declarations, context, and call-site magic
  constant values.

The same cache is used by the public eval FFI entry and nested eval/include
execution. Large one-off fragments bypass it instead of occupying global cache
capacity.

## Linking and targets

`RuntimeFeatures::eval_scope` emits only the core scope helpers.
`RuntimeFeatures::eval_bridge` additionally links PCRE2 and
`libelephc_magician.a`. The bridge is registered in `src/linker.rs` as
`--with-eval` with the optional `ELEPHC_MAGICIAN_LIB_DIR` archive-directory
override. Normal compilation derives the feature automatically; `--with-eval`
force-loads the archive and increases binary size but does not alter AOT
eligibility.

All eval lowering and bridge ABI paths are target-aware and covered by
dedicated integration shards on macOS ARM64, Linux ARM64, and Linux x86_64.
Shared lowering must use the existing ABI helpers rather than assume a specific
register set or object format.

## Documentation ownership

The exhaustive language subset, reflection behavior, builtins, and known gaps
belong in [Eval](../php/eval.md). Per-builtin AOT/eval availability is generated
from the `builtin!` and `eval_builtin!` registries in the
[Builtin Reference](../php/builtins.md). [The Runtime](the-runtime.md) documents
the shared `__rt_*` assembly families and links here for the optional eval
boundary.
