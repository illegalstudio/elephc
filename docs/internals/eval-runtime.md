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
| Interpreter fallback | A dynamic string or a literal requiring dynamic declarations, includes, references, dynamic calls, or another unsupported AOT shape | `eval_bridge`, synchronized scopes, and `elephc_magician`; optional capabilities such as regex are linked separately |

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

Magician has no direct PCRE2 symbols in its base static library. If the final
EIR module requires the regex runtime, generated eval setup registers the
managed `elephc_pcre2_v1_*` shim callbacks before creating the context.
Magician then exposes its regex builtin area through that opaque provider. With
no provider, `preg_*` names are absent from dynamic eval lookup and calling one
fails at runtime. Opaque dynamic source can opt into that capability with
`--with-regex`; visible static regex use enables it through normal feature
detection.

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

`RuntimeCellHandle` also carries Rust-only provenance. A handle returned from
durable context, scope, constant, property, or class storage is borrowed; a
handle created by a constructor, copy, retain, or ownership-transferring result
API is owned. This marker never crosses the C ABI. Raw argument and mutable
reference slots contain contiguous `RuntimeCell *` values, and FFI entrypoints
reconstruct raw pointers with owned provenance by default. APIs that transfer
one owner to Magician keep that default; FFI entrypoints receiving caller-owned
inputs explicitly convert the reconstructed handle to borrowed provenance.

Runtime string contexts that encounter an eval-declared object use
`__elephc_eval_string_context`. The bridge returns the same `ElephcEvalResult`
value-or-throwable shape as method calls: successful strings are persisted while
the formatter copies them, and escaped `__toString()` exceptions enter the native
`__rt_throw_current` path so surrounding compiled `try`/`catch` blocks remain
authoritative.

Scope setters retain the value stored in the context; getters expose borrowed
handles. An expression consumer that requires an owned PHP value copies that
borrowed cell through the shared scope-copy helper. Arrays retain their cursor
and alias side metadata while receiving an independent copy-on-write cell;
objects and resources preserve their PHP identity. Normal returns, runtime
fatals, thrown values, early fragment returns, and function cleanup must all
balance those cells. Persistent declarations and metadata live in the eval
context until its owning generated function or process scope is destroyed.

Expression operands and call arguments use owner ledgers. They evaluate in PHP
source order, pin each temporary before later side effects can replace its
source storage, preserve named, spread, and by-reference targets through
binding, and release only owned entries after invocation or failure cleanup. A
borrowed result is promoted before its supplying ledger is released. Pointer
identity is used only to preserve related side metadata, never to infer whether
a value is owned.

Builtin lookup is also shared at the contract boundary. Magician joins its
implementation hooks to the same `BuiltinId` used by the compiler. For compatible
boxed-cell operations it dispatches a typed `RuntimeBuiltinId` through
`__elephc_runtime_builtin_call_v1`; arguments are borrowed and a successful result
transfers one fresh cell through `result_out`. Unknown IDs and unsupported arities
fail closed. The compiler emits target-aware wrappers for macOS ARM64, Linux ARM64,
and Linux x86_64. By-reference/lvalue, callable, reflection, resource,
eval-declaration, and partial-signature behavior remains on an explicit Magician
adapter with a catalog-audited reason.

Direct builtin calls pass their lexical scope through the existing registry
values hook. This lets callable adapters such as `call_user_func*`, array
callback builtins, and `preg_replace_callback` resolve `self`, `parent`, and
`static` with the caller's method scope after common argument binding. Callable
dispatch with already materialized values deliberately omits that scope, so a
nested callback does not inherit unrelated source-call resolution state.

Eval array reads use a dedicated owned shared-cell mode. Unlike an ordinary
PHP array read, which detaches a boxed zval to preserve value semantics, the
bridge must retain the exact stored cell because that handle can be the
writeback target for an AOT by-reference method, constructor, reflection, or
callable invocation. This mode is separate from the nested-write fetch, which
COW-normalizes the outer container and detaches the selected zval before
mutation.

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
`RuntimeFeatures::eval_bridge` additionally links `libelephc_magician.a`, but
not PCRE2. `RuntimeFeatures::regex` independently resolves managed PCRE2 and
causes eval setup to register the provider callbacks. The bridge is registered
in `src/linker/` as `--with-eval` with the optional
`ELEPHC_MAGICIAN_LIB_DIR` archive-directory override. Normal compilation
derives both features independently; `--with-eval` force-loads the archive,
while `--with-regex` force-enables the regex runtime for opaque source. Neither
flag alters AOT eligibility.

All eval lowering and bridge ABI paths are target-aware and covered by
dedicated integration shards on macOS ARM64, Linux ARM64, and Linux x86_64.
Shared lowering must use the existing ABI helpers rather than assume a specific
register set or object format.

## Documentation ownership

The exhaustive language subset, reflection behavior, builtins, and known gaps
belong in [Eval](../php/eval.md). Per-builtin AOT/eval availability is generated
from the shared `elephc-builtin-contract` catalog joined to the `builtin!` and
`eval_builtin!` backend registries in the
[Builtin Reference](../php/builtins.md). [The Runtime](the-runtime.md) documents
the shared `__rt_*` assembly families and links here for the optional eval
boundary.
