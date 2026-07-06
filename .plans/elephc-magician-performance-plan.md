# elephc-magician Performance Plan

| Order | Work item | Objective | Status | Notes |
|---:|---|---|---|---|
| 1 | Dedicated magician benchmark suite | Measure real hot spots before larger interpreter or bridge refactors | Done | Added `scripts/benchmark_magician.py` plus fixture cases under `benchmarks/magician/cases/`; CI now publishes magician benchmark artifacts. |
| 2 | Eval fragment parse cache | Avoid repeated tokenization and parsing for identical runtime source bytes | Done | Implemented in commit `4ba0efb5c` through `crates/elephc-magician/src/parse_cache.rs`. |
| 3 | Parse-error cache | Avoid reparsing repeated invalid fragments | Done | Included in the same parse cache as successful `EvalProgram` results. |
| 4 | Dynamic context/scope preservation for cached parses | Ensure caching does not freeze magic constants, variables, declarations, or runtime state | Done | The cache stores only immutable parse results; execution still receives the current `ElephcEvalContext` and `ElephcEvalScope`. |
| 5 | Include parse cache | Avoid repeated parsing of identical PHP code blocks loaded through include/require | Done | Added a metadata-validated include-file cache for file bytes and PHP block splitting; include_once state remains context-local and stale-prone missing/path checks stay live. |
| 6 | Eval symbol lookup cache | Speed up repeated function/class/method lookup for symbols declared by eval | Done | Added per-context caches for dynamic/native functions, constants, class-like symbols, aliases, and eval class methods. |
| 7 | Direct builtin dispatch | Avoid repeated string matching and generic dispatch for common builtins | Done | Added a hot positional direct-dispatch layer for benchmark-relevant scalar/core builtins while named/spread calls still use generic binding. |
| 8 | Callable resolution cache | Avoid reconstructing equivalent string/array/first-class/closure callables repeatedly | Done | Added bounded stable string-callable normalization caching; object, array, first-class, and scope-sensitive special-class callables still resolve live. |
| 9 | Reduce `RuntimeValueOps` calls on simple operations | Cut bridge overhead for arithmetic, comparisons, casts, and simple output paths | Done | Added a combined non-object echo bridge so scalar/null/array output avoids the separate `type_tag` call; arithmetic coercion shortcuts remain deferred to unboxed scalar work. |
| 10 | Unboxed scalar fast paths | Avoid boxing/unboxing for hot int/float/bool/string paths inside eval execution | Done | Added a conservative int/bool temporary evaluator for pure assignment, return, and condition boundaries; scope cells remain boxed and risky coercions fall back to runtime hooks. |
| 11 | Compact bytecode or linear EvalIR form | Reduce tree-walk overhead and branch-heavy dispatch in the current EvalIR interpreter | Done | Added an optional cached straight-line linear executable with a small stack VM; loops, declarations, includes, OOP, short-circuit expressions, and other complex forms stay on EvalIR fallback. |
| 12 | Array/reference/COW bridge optimizations | Reduce cost of array mutation and by-reference parameter handling | Done | Added narrow integer-key array write fast paths for eval scope-variable indexed append/set; associative append, references, and broad COW behavior remain on the existing semantic path. |
| 13 | AOT for literal `eval` | Compile `eval('...')` fragments ahead of time instead of interpreting them through magician | Done | Added conservative literal AOT for scalar constants, scalar output/returns/stores, and boxed Mixed scope reads/read-modify-writes; unsupported builtins, declarations, includes, OOP, and control flow keep the bridge fallback. |

## 1. Dedicated Magician Benchmark Suite

### Goal

Create repeatable benchmarks that isolate where `elephc-magician` spends time before making larger performance changes. This should prevent optimizing only the obvious parse path while missing the real cost of interpreter dispatch, value boxing, builtin lookup, callable resolution, or array/reference handling.

### Scope

Add a small benchmark harness that compares:

- Native elephc code without eval.
- elephc code that enters magician through `eval`.
- PHP standard execution without eval.
- PHP execution through `eval`.

The suite should include both microbenchmarks and a few mixed workloads:

- Repeated identical small `eval` fragments.
- One large `eval` fragment with a compute-heavy loop.
- Arithmetic-only loops.
- String concatenation and output.
- Array reads/writes.
- Function calls declared inside eval.
- Builtin calls inside eval.
- Callable dispatch inside eval.
- Include/require with repeated code blocks.

### Current State

Done.

The implementation adds:

- `scripts/benchmark_magician.py`
- `benchmarks/magician/README.md`
- `benchmarks/magician/cases/*`

The suite compares native elephc, elephc through magician eval, native PHP, and
PHP through eval. It records runtime wall clock, eval invocation counts,
fragment sizes, literal-vs-dynamic source shape, parse-cache expectations, and
stdout correctness. The existing benchmark CI job also runs the magician suite
and uploads markdown/JSON artifacts.

### Likely Files

- `benches/` or `scripts/bench-eval-*` depending on existing project conventions.
- `tests/codegen/eval*.rs` only for correctness guards, not timing.
- `crates/elephc-magician/src/` only if benchmark-only hooks are required behind `#[cfg(test)]` or a feature gate.

### Validation

Each benchmark should record:

- Runtime wall clock.
- Number of eval invocations.
- Fragment size.
- Whether the fragment is literal or dynamic.
- Whether parse cache should hit.
- Output correctness against PHP where practical.

Validation run after implementation:

- `python3 -m py_compile scripts/benchmark_magician.py`
- `python3 scripts/benchmark_magician.py --list`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case repeated_small_eval --json /tmp/elephc-magician-bench.json --markdown /tmp/elephc-magician-bench.md`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --json /tmp/elephc-magician-bench-all.json --markdown /tmp/elephc-magician-bench-all.md`

### Risks

Benchmark results can be misleading if compile+assemble+link time is included for runtime comparisons. Runtime-only binaries should be generated once and executed repeatedly.

## 2. Eval Fragment Parse Cache

### Goal

Avoid repeated tokenization and parsing for identical eval source bytes.

### Current State

Done in commit `4ba0efb5c`.

The implementation adds `crates/elephc-magician/src/parse_cache.rs` and routes these call sites through it:

- `crates/elephc-magician/src/ffi/execute.rs`
- `crates/elephc-magician/src/interpreter/include_exec.rs`

### Design

The cache is process-local, bounded, and keyed by exact fragment bytes. It stores immutable `EvalProgram` instances behind `Arc`, plus parse errors.

Current policy:

- FIFO capacity: 256 entries.
- Maximum cacheable source: 64 KiB.
- Larger fragments bypass the cache.
- Mutex poisoning is recovered by taking the inner cache.

### Validation Already Run

- `cargo test -p elephc-magician parse_cache`
- `cargo test -p elephc-magician execute_program_nested_eval_uses_same_scope`
- `cargo test -p elephc-magician execute_program_include_uses_call_site_and_returns_file_result`
- `cargo test --test codegen_tests test_eval_return_value`
- `git diff --check`

### Follow-Up

After benchmarks exist, revisit capacity and maximum source size. If workloads show many repeated fragments above 64 KiB, consider size-based memory budgeting instead of a hard source-length cutoff.

## 3. Parse-Error Cache

### Goal

Avoid reparsing invalid fragments that are repeatedly passed to `eval`.

### Current State

Done as part of the parse cache.

### Design

The cached result type is:

```rust
Result<Arc<EvalProgram>, EvalParseError>
```

This means both successful parses and parse errors are reusable.

### Validation Already Run

The unit test `parse_cache::tests::cache_reuses_parse_errors` verifies that parse errors are cached and returned without reparsing.

### Follow-Up

If user code frequently emits many distinct invalid fragments, error caching could retain noise. Benchmark and memory telemetry should decide whether invalid-fragment caching needs a lower capacity or should be disabled for very large invalid inputs.

## 4. Dynamic Context And Scope Preservation

### Goal

Guarantee the parse cache does not alter PHP-observable runtime behavior.

The cache must not freeze:

- Variables from the caller scope.
- Variables created by eval.
- Function/class/interface/trait/enum declarations.
- Magic constants that depend on the current call site.
- Include file metadata.
- Pending throw state.
- Return values.

### Current State

Done for the parse cache.

### Design

The cache stores only the parsed `EvalProgram`. Execution still happens through the existing interpreter entry points and receives the current context and scope every time.

Magic constants remain safe because EvalIR stores magic-constant nodes and runtime evaluation resolves context-dependent values through `ElephcEvalContext`.

### Validation Already Run

- Nested eval scope sharing: `execute_program_nested_eval_uses_same_scope`.
- Include file magic and scope sharing: `execute_program_include_uses_call_site_and_returns_file_result`.
- Native bridge return value: `test_eval_return_value`.

### Follow-Up

Add a focused regression test for the same cached fragment executed under two different call sites, verifying `__FILE__` and `__DIR__` remain context-sensitive.

## 5. Include Parse Cache

### Goal

Avoid reparsing identical PHP code blocks loaded through include/require.

### Current State

Done.

The parse cache is used by `eval_execute_include_code()`, so parsed PHP code
blocks are reused when exact source bytes match.

The implementation now also adds a bounded process-local include-file cache in
`crates/elephc-magician/src/interpreter/include_exec.rs`. Cache entries store:

- Canonical include key for `include_once`/`require_once`.
- File metadata used to reject stale entries.
- File bytes.
- Precomputed raw/PHP-code block ranges for `<?php ... ?>` split scanning.

The cache deliberately keeps missing-file checks, cwd-first resolution checks,
and caller-directory fallback checks live. Caching those negative/path decisions
would hide files created later at runtime or violate PHP's cwd-first include
order. `include_once` state remains in `ElephcEvalContext`.

### Likely Files

- `crates/elephc-magician/src/interpreter/include_exec.rs`
- Possibly `crates/elephc-magician/src/context.rs` for include-once metadata if shared caching needs context-level state.

### Implementation Plan

1. Measure include-heavy workloads first.
2. Add a small include-file cache only if file I/O dominates.
3. Key file cache entries by canonical path plus file metadata where available.
4. Keep include_once semantics in `ElephcEvalContext`; do not move "already included" behavior into a global cache.
5. Ensure `__FILE__` and `__DIR__` still come from the current include path, not from cached source metadata.

All five steps are implemented. The benchmark suite added in point 1 includes
`include_repeated`, and the cache uses the current resolved include path when
executing cached bytes so magic constants remain call-site sensitive.

### Validation

Run focused include tests:

- `execute_program_include_uses_call_site_and_returns_file_result`
- `execute_program_include_once_skips_regularly_included_file`
- `execute_program_missing_include_warns_and_returns_false`
- `execute_program_missing_require_is_runtime_fatal`

Validation run after implementation:

- `cargo test -p elephc-magician execute_program_include_uses_call_site_and_returns_file_result`
- `cargo test -p elephc-magician execute_program_include_once_skips_regularly_included_file`
- `cargo test -p elephc-magician execute_program_regular_include_observes_modified_file_after_cache_hit`
- `cargo test -p elephc-magician execute_program_missing_include_warns_and_returns_false`
- `cargo test -p elephc-magician execute_program_missing_require_is_runtime_fatal`
- `cargo test --test codegen_tests test_eval_fragment_include_once_and_plain_file`
- `cargo test --test codegen_tests test_eval_fragment_include_executes_php_file_and_returns_value`
- `cargo test --test codegen_tests test_eval_fragment_missing_require_fails`

### Risks

File caches can easily become stale. A conservative first version should cache parsed source bytes only within one process and avoid hiding file changes unless PHP-compatible behavior is explicitly defined.

## 6. Eval Symbol Lookup Cache

### Goal

Speed up repeated lookup of functions, classes, methods, interfaces, traits, and enums declared dynamically through eval.

### Motivation

Once parsing is cached, repeated dynamic calls may still pay for name normalization, case-insensitive matching, namespace fallback, and symbol-table scans.

### Likely Files

- `crates/elephc-magician/src/context.rs`
- `crates/elephc-magician/src/interpreter/dynamic_functions.rs`
- `crates/elephc-magician/src/interpreter/reflection.rs`
- `crates/elephc-magician/src/interpreter/statements.rs`

### Implementation Plan

1. Inventory current lookup paths for function, class, method, and constant resolution.
2. Identify whether each lookup is already stored in a normalized map.
3. Add cache layers only where repeated lookup still performs normalization or scanning.
4. Invalidate or update caches when eval declares a new symbol.
5. Keep case-insensitive PHP behavior canonical.
6. Preserve namespace fallback for builtins and user symbols.

### Current State

Done.

The implementation adds a per-context `EvalSymbolLookupCache` in
`crates/elephc-magician/src/context.rs`. It caches:

- Dynamic/native function classification.
- Dynamic constant key resolution.
- Class-like resolution for classes, interfaces, traits, enums, and aliases.
- Inherited and directly declared eval class method lookups.

The cache is protected by a poison-recovering `Mutex` so FFI `catch_unwind`
wrappers remain unwind-safe. Declarations and alias registration clear the
affected cache families, so cached misses do not hide later eval declarations.

### Validation

Add tests for:

- Repeated calls to an eval-declared function.
- Case-insensitive lookup.
- Namespaced calls with builtin fallback.
- `function_exists`, `class_exists`, and reflection seeing updated declarations after eval.

Validation run after implementation:

- `cargo test -p elephc-magician function_exists_sees_function_declared_after_cached_miss`
- `cargo test -p elephc-magician constant_exists_sees_constant_defined_after_cached_miss`
- `cargo test -p elephc-magician dynamic_class_exists_sees_alias_declared_after_cached_miss`
- `cargo test -p elephc-magician class_method_lookup_sees_class_declared_after_cached_miss`
- `cargo test -p elephc-magician function_exists_reports_declared_eval_function`
- `cargo test -p elephc-magician dynamic_class_exists_reports_declared_eval_class`
- `cargo test -p elephc-magician execute_program_class_alias_registers_aliases`
- `cargo test -p elephc-magician execute_program_static_callable_array_dispatches_eval_method`
- `cargo test --test codegen_tests test_function_exists_sees_builtins_and_eval_declared_functions_after_eval`
- `cargo test --test codegen_tests test_eval_declared_function_can_be_called_with_call_user_func`
- `cargo test --test codegen_tests test_eval_function_exists_builtin_case_insensitive`

### Risks

Incorrect caching here can make declarations invisible or make duplicate declarations appear valid. This must be treated as a semantic change, not a pure optimization.

## 7. Direct Builtin Dispatch

### Goal

Avoid repeated generic builtin lookup and string matching for common builtin calls inside eval.

### Motivation

Eval currently supports many builtins through interpreter dispatch. If a hot loop repeatedly calls the same builtin, the dispatch path should not repeatedly resolve the same function name from scratch.

### Likely Files

- `crates/elephc-magician/src/interpreter/builtins/`
- `crates/elephc-magician/src/interpreter/core_builtins.rs`
- `crates/elephc-magician/src/interpreter/builtin_metadata.rs`
- `crates/elephc-magician/src/interpreter/dynamic_functions.rs`

### Implementation Plan

1. Use benchmarks to rank builtin categories by runtime cost.
2. Add a compact builtin id or function pointer to parsed call expressions when safe.
3. Keep unknown/dynamic call paths generic.
4. Preserve PHP case-insensitivity and namespace fallback.
5. Ensure direct dispatch and generic dispatch share argument validation.

### Current State

Done.

The benchmark suite's `builtin_calls` case stresses repeated `strlen()` and
`intval()` inside cached eval fragments, so the first direct path targets that
hot scalar/core family. The implementation adds
`crates/elephc-magician/src/interpreter/builtins/registry/direct.rs` with a
compact `EvalDirectBuiltin` selector for:

- `strlen`
- `intval`, `floatval`, `strval`, `boolval`
- `count`
- `ord`
- `abs`

The fast path only accepts plain positional direct calls. Named arguments,
spread arguments, dynamic callables, first-class callables, and unknown names
fall through to the existing generic builtin binding/dispatch path, so argument
validation and callable semantics remain centralized.

### Validation

For each optimized builtin group, add parity tests for:

- Direct call.
- Case-insensitive call.
- Namespaced fallback.
- Named arguments when supported.
- First-class callable and callable aliases when relevant.

Validation run after implementation:

- `cargo test -p elephc-magician execute_program_dispatches_hot_direct_builtins_with_generic_fallbacks`
- `cargo test -p elephc-magician execute_program_dispatches_cast_builtins`
- `cargo test -p elephc-magician execute_program_dispatches_abs_builtin`
- `cargo test -p elephc-magician execute_program_dispatches_ord_builtin`
- `cargo test -p elephc-magician execute_program_counts_eval_countable_objects`
- `cargo test -p elephc-magician execute_program_namespace_call_falls_back_to_builtin`
- `cargo test -p elephc-magician execute_program_namespace_function_overrides_builtin_fallback`
- `cargo test -p elephc-magician execute_program_first_class_callables_dispatch_functions_and_methods`
- `cargo test --test codegen_tests test_eval_dispatches_simple_builtin_calls`
- `cargo test --test codegen_tests test_eval_dispatches_cast_builtin_calls`
- `cargo test --test codegen_tests test_eval_dispatches_abs_builtin_call`
- `cargo test --test codegen_tests test_namespaced_calls_fall_back_to_builtin_before_and_after_eval`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case builtin_calls --json /tmp/elephc-magician-builtin-bench.json --markdown /tmp/elephc-magician-builtin-bench.md`

### Risks

A second builtin registry can drift from the canonical catalog. Any direct dispatch table must be generated from or tightly tied to the existing metadata source.

## 8. Callable Resolution Cache

### Goal

Avoid rebuilding equivalent callable targets repeatedly.

### Motivation

Callable semantics now cover string callables, array callables, first-class callable syntax, and closures. Repeatedly resolving the same target can become expensive in callback-heavy eval programs.

### Likely Files

- `crates/elephc-magician/src/interpreter/dynamic_functions.rs`
- `crates/elephc-magician/src/interpreter/reflection.rs`
- `crates/elephc-magician/src/context.rs`
- `crates/elephc-magician/src/ffi/callables.rs`

### Implementation Plan

1. Define a canonical callable key for stable cases.
2. Cache only callables whose target is stable under current context rules.
3. Invalidate or bypass on symbol-table changes when needed.
4. Keep bound object callables separate from static function/class callables.
5. Do not cache by raw runtime-cell pointer unless ownership and lifetime are explicit.

### Current State

Done.

The implementation adds a bounded per-context string-callable normalization cache
to `ElephcEvalContext`. Cached entries cover stable callback strings only:

- Function strings such as `"strlen"` or `"eval_declared_fn"`.
- Ordinary static method strings such as `"ClassName::method"`.

The cache stores normalized `Named` or `StaticMethod` callable metadata, not a
validated callable target. Invocation and `is_callable()` still perform live
symbol/method checks, so cached misses do not hide later eval declarations.

These forms deliberately bypass the cache:

- Object callables.
- Callable arrays.
- First-class callable/Closure objects.
- Scope-sensitive `self::`, `static::`, and `parent::` string callables.

That keeps `$this`, called-class, lexical scope, and runtime-cell lifetime rules
owned by the existing live resolver.

### Validation

Add tests for repeated:

- String function callables.
- Static string callables like `Class::method`.
- Array callables.
- First-class callables.
- `Closure::fromCallable`.
- By-reference callable arguments.

Validation run after implementation:

- `cargo test -p elephc-magician execute_program_string_callable_cache_sees_late_function_declaration`
- `cargo test -p elephc-magician execute_program_string_callable_cache_sees_late_static_method_declaration`
- `cargo test -p elephc-magician execute_program_call_user_func_dispatches_builtin`
- `cargo test -p elephc-magician execute_program_callable_array_variable_dispatches_object_method`
- `cargo test -p elephc-magician execute_program_static_callable_array_dispatches_eval_method`
- `cargo test -p elephc-magician execute_program_first_class_callables_dispatch_functions_and_methods`
- `cargo test -p elephc-magician execute_program_static_runtime_callables_write_back_by_ref_type_coercion`
- `cargo test --test codegen_tests test_eval_closure_from_callable_special_string_callables_preserve_by_ref_writeback`
- `cargo test --test codegen_tests test_eval_declared_callable_forms_preserve_by_ref_writeback`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case callable_dispatch --json /tmp/elephc-magician-callable-bench.json --markdown /tmp/elephc-magician-callable-bench.md`

### Risks

Bound method callables can carry `$this`, visibility scope, or closure binding state. Caching must not reuse a callable across an incompatible object or visibility context.

## 9. Reduce `RuntimeValueOps` Calls On Simple Operations

### Goal

Reduce the number of runtime bridge calls needed for simple operations inside eval.

### Motivation

Even after parse and dispatch are cached, simple arithmetic can still be slow if each operator repeatedly boxes, unboxes, allocates, or crosses through generic runtime hooks.

### Current State

Done.

The implementation adds `RuntimeValueOps::echo_non_object()` and a generated
`__elephc_eval_value_echo_non_object` bridge for ARM64 and x86_64. Eval `echo`
statements and `print` expressions now try the combined non-object path first:
scalar/null/array output is handled in one runtime bridge call, while object
values still defer to the existing interpreter `__toString()` dispatch before
emitting the result.

Before this change, common scalar output paid one bridge call for `type_tag()`
and one for `echo()`. The fast path now uses one bridge call for the same
non-object cases. PHP numeric operation shortcuts were intentionally left on
the generic runtime hooks because exact scalar coercion and error/fatal
semantics are subtle enough to belong with the unboxed scalar fast-path work.

Benchmark smoke after the change:

```text
python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case string_output --json /tmp/elephc-magician-output-bench.json --markdown /tmp/elephc-magician-output-bench.md
string_output | 1200 evals | elephc native 310.35 ms | elephc eval 395.56 ms | eval/native 1.27x
```

### Likely Files

- `crates/elephc-magician/src/interpreter/expressions.rs`
- `crates/elephc-magician/src/interpreter/constant_eval.rs`
- `crates/elephc-magician/src/interpreter/runtime_ops.rs`
- `crates/elephc-magician/src/runtime_hooks/ops.rs`

### Implementation Plan

1. Count `RuntimeValueOps` calls in arithmetic-heavy eval benchmarks.
2. Identify pure scalar operations where both operands are already scalar cells.
3. Add internal helper paths that perform combined operation + allocation where safe.
4. Avoid changing behavior for arrays, objects, strings with PHP coercion edge cases, refs, or `mixed` values until covered.
5. Keep fatal/error behavior identical to the current generic path.

### Validation

Validation run after implementation:

- `cargo test -p elephc-magician execute_program_print_returns_one`
- `cargo test -p elephc-magician execute_program_echoes_and_unsets_scope_value`
- `cargo test -p elephc-magician execute_program_evaluates_division_and_modulo`
- `cargo test -p elephc-magician execute_program_evaluates_exponentiation`
- `cargo test -p elephc-magician execute_program_echoes_comma_list`
- `cargo test --test codegen_tests test_eval_output_fast_path_preserves_scalar_and_object_echo`
- `cargo test --test codegen_tests test_eval_scalar_add_executes_through_bridge`
- `./scripts/test-linux-x86_64.sh test_eval_output_fast_path_preserves_scalar_and_object_echo`
- `./scripts/test-linux-arm64.sh test_eval_output_fast_path_preserves_scalar_and_object_echo`
- `git diff --check`

### Risks

PHP scalar coercion is subtle. Every fast path needs either exact compatibility or an explicit fallback to the current generic path.

## 10. Unboxed Scalar Fast Paths

### Goal

Avoid boxing and unboxing hot scalar values inside eval loops.

### Motivation

Compute-heavy eval programs are likely dominated by repeated scalar loads, arithmetic, comparisons, and stores. Keeping scalars in a compact internal representation can reduce allocation and bridge overhead.

### Current State

Done.

The implementation adds `crates/elephc-magician/src/interpreter/unboxed.rs`,
which evaluates small pure int/bool expression trees into temporary
`EvalUnboxedScalar` values. The fast path is wired only at explicit boundaries:

- `StoreVar` boxes once before writing the scope cell.
- `return expr` boxes once before returning.
- `if`, `while`, `do/while`, and `for` conditions use unboxed truthiness when
  the condition is a pure supported scalar expression.

Scope-visible values remain runtime cells. The unboxed evaluator reads only
visible int/bool scope cells and handles checked integer add/sub/mul/mod,
integer comparisons, same-kind equality, logical not/and/or/xor, and integer
unary plus/minus. Overflow, division/modulo-by-zero, strings, floats, arrays,
objects, calls, refs, and other PHP-coercion-sensitive cases return `None` and
fall back to the existing runtime hooks.

Benchmark smoke after the change:

```text
python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case arithmetic_loop --json /tmp/elephc-magician-arithmetic-bench.json --markdown /tmp/elephc-magician-arithmetic-bench.md
arithmetic_loop | 4000 evals | elephc native 296.28 ms | elephc eval 396.30 ms | eval/native 1.34x
```

### Likely Files

- `crates/elephc-magician/src/value.rs`
- `crates/elephc-magician/src/interpreter/expressions.rs`
- `crates/elephc-magician/src/interpreter/statements.rs`
- `crates/elephc-magician/src/interpreter/scope_cells.rs`
- `crates/elephc-magician/src/runtime_hooks/`

### Implementation Plan

1. Introduce an interpreter-local value enum for hot temporaries, not persistent scope cells.
2. Keep scope-visible values as runtime cells unless ownership semantics are fully modeled.
3. Add unboxed paths for integer and boolean first.
4. Add float after edge cases are verified against PHP.
5. Add string only for immutable literals or clearly owned strings.
6. Box only when a value escapes to scope, output, by-ref parameters, arrays, objects, or runtime hooks.

### Validation

Validation run after implementation:

- `cargo test -p elephc-magician execute_program_unboxes_integer_store_expression_until_assignment`
- `cargo test -p elephc-magician execute_program_unboxed_integer_store_falls_back_for_modulo_by_zero`
- `cargo test -p elephc-magician execute_program_evaluates_compound_assignments`
- `cargo test -p elephc-magician execute_program_evaluates_division_and_modulo`
- `cargo test --test codegen_tests test_eval_unboxed_integer_store_expression_executes_through_bridge`
- `cargo test --test codegen_tests test_eval_division_modulo_execute_through_bridge`
- `cargo test --test codegen_tests test_eval_for_loop_uses_less_than_condition`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case arithmetic_loop --json /tmp/elephc-magician-arithmetic-bench.json --markdown /tmp/elephc-magician-arithmetic-bench.md`
- `git diff --check`

### Risks

The danger is splitting magician into two incompatible value systems. The unboxed layer should be a temporary execution optimization with explicit boxing boundaries.

## 11. Compact Bytecode Or Linear EvalIR Form

### Goal

Reduce tree-walk and branch-heavy interpreter dispatch overhead.

### Motivation

The current EvalIR is a structured tree. A compact linear representation can improve cache locality and simplify dispatch, especially for loops.

### Likely Files

- `crates/elephc-magician/src/eval_ir.rs`
- New module such as `crates/elephc-magician/src/eval_bytecode.rs`
- `crates/elephc-magician/src/interpreter/`
- `crates/elephc-magician/src/parser/`

### Implementation Plan

1. Do not replace EvalIR immediately.
2. Add an optional lowering step from `EvalProgram` to a compact executable form.
3. Cache the lowered executable form alongside or inside the parse cache.
4. Start with expression-heavy straight-line code.
5. Add loops and control flow after linear basic blocks are proven.
6. Keep declarations and complex OOP constructs on the existing EvalIR path until needed.

### Current State

Done.

The implementation adds `crates/elephc-magician/src/eval_linear.rs` and stores
`CachedEvalFragment { program, linear }` entries in the parse cache. Linear
lowering is intentionally conservative and currently accepts straight-line
fragments made of assignment, echo, print, expression statements, and return
over constants, variable loads, unary operators, and non-short-circuit binary
operators.

The interpreter executes the lowered form through a small stack machine in
`crates/elephc-magician/src/interpreter/statements.rs`. Int/bool stack values
stay unboxed where the point-10 scalar rules apply, and values are boxed at
store, echo, pop, and return boundaries. Unsupported fragments continue through
the existing EvalIR interpreter.

### Validation

Run parity tests with both execution engines if a temporary dual path exists:

- Existing magician interpreter unit tests.
- Focused codegen eval tests.
- PHP cross-checks for edge cases.

Validation run after implementation:

- `cargo test -p elephc-magician linear_lowering`
- `cargo test -p elephc-magician execute_prepared_program_uses_linear_unboxed_scalar_path`
- `cargo test --test codegen_tests test_eval_linear_cached_fragment_and_evalir_fallback_execute_through_bridge`
- `cargo test --test codegen_tests test_eval_unboxed_integer_store_expression_executes_through_bridge`
- `cargo test --test codegen_tests test_eval_for_loop_uses_less_than_condition`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case repeated_small_eval --json /tmp/elephc-magician-linear-bench.json --markdown /tmp/elephc-magician-linear-bench.md`

Benchmark smoke result:

- `repeated_small_eval`: 5000 evals, elephc native `329.19 ms`, elephc eval `378.11 ms`, eval/native `1.15x`.

### Risks

This is a larger architectural change. It should not happen before benchmark data proves tree dispatch is a real bottleneck after parse caching and scalar fast paths.

## 12. Array, Reference, And COW Bridge Optimizations

### Goal

Reduce overhead for array mutation, by-reference parameters, and copy-on-write behavior.

### Motivation

Array and reference-heavy eval code can be expensive because correctness requires preserving PHP aliasing, reference cells, and COW rules.

### Likely Files

- `crates/elephc-magician/src/interpreter/array_literals.rs`
- `crates/elephc-magician/src/interpreter/scope_cells.rs`
- `crates/elephc-magician/src/interpreter/statements.rs`
- `crates/elephc-magician/src/runtime_hooks/`
- `crates/elephc-magician/src/ffi/`

### Implementation Plan

1. Benchmark array mutation and by-ref workloads separately.
2. Identify repeated helper patterns that can be fused safely.
3. Optimize append/set/get paths before broad COW changes.
4. Keep reference binding and mutation behavior covered by regression tests.
5. Add cleanup tests for normal return, fatal, and throwable paths.

### Current State

Done for the low-risk indexed-array write subset.

The implementation adds `RuntimeValueOps::array_set_int_index()` with a
production bridge wrapper, `__elephc_eval_value_array_set_int_index`, emitted
for both ARM64 and x86_64. Eval scope-variable writes now use this hook when:

- `$array[] = value` targets an indexed array or a newly created array. The
  current length is read before evaluating the RHS, preserving the existing
  source-order behavior.
- `$array[$i] = value` has a pure unboxed integer index.

Associative-array append still uses `eval_array_append_key()` so PHP's
largest-integer-key behavior is preserved. String keys, side-effecting index
expressions, object `ArrayAccess`, by-reference writeback, property/static
array writes, and broad COW/reference handling continue through the existing
generic paths.

### Validation

Add tests for:

- Array append and indexed set.
- Associative key set.
- By-ref function and method parameters.
- Ref-like builtin parameters.
- Aliasing across eval and native code.
- Cleanup after fatal and uncaught throwable.

Validation run after implementation:

- `cargo test -p elephc-magician execute_program_appends_indexed_scope_array_with_direct_int_key_write`
- `cargo test -p elephc-magician execute_program_sets_scope_array_integer_indexes_with_direct_int_key_write`
- `cargo test -p elephc-magician execute_program_appends_assoc_scope_array`
- `cargo test --test codegen_tests test_eval_indexed_array_append_is_visible_after_eval`
- `cargo test --test codegen_tests test_eval_indexed_array_write_is_visible_after_eval`
- `cargo test --test codegen_tests test_eval_assoc_array_append_uses_php_next_key`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case array_reads_writes --json /tmp/elephc-magician-array-bench.json --markdown /tmp/elephc-magician-array-bench.md`

Benchmark smoke result:

- `array_reads_writes`: 1 eval, elephc native `348.16 ms`, elephc eval `403.57 ms`, eval/native `1.16x`.

### Risks

This area has high semantic risk. Incorrect optimization can create stale aliases, missed mutations, double frees, or leaks.

## 13. AOT For Literal `eval`

### Goal

Compile literal eval fragments ahead of time so `eval('...')` can bypass the magician interpreter where possible.

### Current State

Done for the conservative eligibility slice implemented in this plan.

Literal eval calls still lower to the EIR opcode `EvalLiteralCall`, but the
backend now parses the literal fragment at compile time and directly emits
native code for eligible fragments. The current AOT subset accepts scalar
constants, simple scalar arithmetic, scalar concat, scalar `echo` / `print`,
scalar `return`, scalar variable stores, and boxed Mixed scope reads used by
read-modify-write expressions. Unsupported fragments keep the bridge fallback
to `__elephc_eval_execute`.

### Motivation

This is the largest potential speedup for literal eval because the code can become normal native elephc code instead of runtime-parsed and interpreted code.

### Likely Files

- `src/ir/`
- `src/ir_lower/expr/`
- `src/codegen_ir/lower_inst/`
- `src/types/`
- `src/name_resolver/`
- `src/resolver/`
- `src/optimize/`
- `crates/elephc-magician/` only for fallback and compatibility.

### Implementation Plan

1. Keep the existing fallback bridge as the compatibility path. Done.
2. Parse literal fragments at compile time. Done for the scalar AOT subset.
3. Run the same frontend passes needed for normal PHP code where possible.
   Deferred for broader eligibility; the current subset accepts only syntax
   that can be evaluated conservatively without cross-pass rewrites.
4. Reject or fall back for constructs that require runtime-only context. Done.
5. Lower eligible literal eval fragments into native codegen. Done for the
   conservative scalar and scope read/write subset through the `EvalLiteralCall`
   backend path.
6. Materialize eval-visible scope reads/writes through the same dynamic-scope
   bridge used by runtime eval. Done for scalar variable stores and boxed Mixed
   scope reads used by supported read-modify-write expressions.
7. Preserve eval return semantics: `return` exits eval, not the caller function.
   Done for scalar returns.
8. Preserve declaration side effects for functions/classes declared by eval.
   Done by falling back for declarations.
9. Add diagnostics or assembly markers showing whether a literal eval used AOT
   or fallback. Done.
10. Expand eligibility gradually. Scope read/read-modify-write expressions are
    now included; builtins, declarations, includes, OOP, control flow, and full
    frontend-pass lowering remain on the compatibility fallback.

### Validation

Add parity tests for:

- Literal eval assigning existing variables. Covered by scalar store and
  existing native visibility tests.
- Literal eval creating variables. Covered.
- Literal eval returning values. Covered.
- Literal eval with output. Covered.
- Literal eval declarations. Covered by fallback behavior; direct declaration
  AOT remains unsupported.
- Literal eval using builtins. Covered by fallback behavior; direct builtin AOT
  remains unsupported.
- Literal eval inside functions and methods. Existing bridge tests cover the
  fallback path; direct scalar AOT uses the same function-local eval scope.
- Fallback when unsupported syntax is present. Covered.
- No magician link for programs without eval. Covered.

Validation run after implementation:

- `cargo test --test codegen_tests test_literal_eval_scalar_return_uses_aot_without_execute_bridge`
- `cargo test --test codegen_tests test_literal_eval_scalar_store_uses_aot_scope_write`
- `cargo test --test codegen_tests test_literal_eval_scope_read_write_uses_aot_without_execute_bridge`
- `cargo test --test codegen_tests test_eval_codegen_requires_eval_bridge`
- `cargo test --test codegen_tests test_dynamic_eval_does_not_emit_literal_aot_marker`
- `cargo test --test codegen_tests test_eval_return_value_is_available_to_native_code`
- `cargo test --test codegen_tests test_eval_created_variable_is_visible_after_eval`
- `cargo test --test codegen_tests test_eval_scope_persists_between_eval_calls`
- `cargo test --test codegen_tests test_eval_can_change_existing_local_type`
- `cargo test --test codegen_tests test_eval_scalar_add_executes_through_aot`
- `cargo test --test codegen_tests test_eval_scalar_echo_executes_through_aot`
- `cargo test --test codegen_tests test_non_eval_program_does_not_request_eval_bridge`
- `cargo test --test codegen_tests test_eval_reads_and_writes_existing_local`
- `cargo test --test codegen_tests test_eval_return_and_scope_write_are_visible`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case literal_scalar_aot --json /tmp/elephc-magician-literal-aot-bench.json --markdown /tmp/elephc-magician-literal-aot-bench.md`
- `python3 scripts/benchmark_magician.py --iterations 1 --warmup 0 --case literal_scope_read_write_aot --json /tmp/elephc-magician-literal-scope-aot-bench.json --markdown /tmp/elephc-magician-literal-scope-aot-bench.md`

Focused benchmark evidence:

- `literal_scalar_aot`: 5000 literal scalar eval returns, eval `408.72 ms`
  versus native `436.25 ms` (`0.94x`).
- `literal_scope_read_write_aot`: 5000 literal eval scope read/write
  invocations, eval `365.63 ms` versus native `316.49 ms` (`1.16x`).

### Risks

AOT eval crosses the static/dynamic boundary. The main risk is accidentally treating eval code as ordinary static code and losing PHP eval semantics for scope, declarations, magic constants, or returns.

## Recommended Milestone Order

1. Keep the completed parse cache as the first performance improvement.
2. Add benchmark coverage before changing interpreter internals.
3. Implement symbol lookup, builtin dispatch, and callable caches while they can still be verified as mostly semantic-preserving optimizations.
4. Use benchmark data to decide between reducing `RuntimeValueOps`, unboxed scalars, or compact bytecode first.
5. Treat array/reference/COW improvements as a separate correctness-heavy milestone.
6. Continue AOT literal eval as the strategic long-term path, using the bridge fallback for all unsupported cases.
