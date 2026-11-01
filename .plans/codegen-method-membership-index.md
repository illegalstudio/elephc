# Codegen method-membership index specification

## Checklist

- [x] Build one immutable instance/static method-membership index per EIR module.
- [x] Route all six audited membership predicates through the shared index.
- [x] Preserve deterministic source-order emission and interface traversal.
- [x] Add differential index tests and focused end-to-end regressions.
- [ ] Measure the large `--web` compile again before considering further optimizations.

## Goal

Replace repeated linear membership scans over `Module::class_methods` during assembly emission with
one immutable module-wide index. This change is only a lookup acceleration: it must preserve the
exact old matching rules, diagnostics, emitted-symbol decisions, and output order.

## Exact index contract

`SharedCodegenState::for_module(&Module)` is the only production constructor. It creates all empty
codegen caches privately, scans `module.class_methods` exactly once, and builds two private
`HashSet<(String, String)>` values: one for instance methods and one for static methods.

For each `Function` in `module.class_methods`, construction must:

1. call `function.name.rsplit_once("::")`;
2. skip the entry when that delimiter is absent;
3. store the raw class prefix returned by `rsplit_once` unchanged, including leading backslashes,
   namespace bytes, and case; and
4. call `php_symbol_key` on the method-name suffix only, then store that canonical method key.

The destination set is selected solely by `function.flags.is_static`. The full `Class::method`
string and the class prefix must never be passed through `php_symbol_key` by this index.

The index is built once from the immutable `&Module` received by `emit_module` and is never mutated
during emission. It represents membership under the old matching rules; it must not replace scans
whose purpose is source-order body emission, metadata construction, generator inspection, or any
other operation beyond the six audited membership predicates.

`SharedCodegenState` must not implement an externally callable empty `Default`. Its private empty
initializer may only be called by `for_module`, which populates the index before the state becomes
observable. `emit_module` must call `SharedCodegenState::for_module(module)`.

## Lookup API and call-site preservation

The two sets remain private. The shared state exposes:

- `emitted_method_contains(class_name, canonical_method_key, is_static)`, which performs one lookup
  in the selected set; and
- a crate-private borrowed view of the instance set for the single interface-layout consumer.

The existing predicate facades receive `class_name` and `method_key` as two already-separated
arguments. They must preserve those arguments exactly and delegate directly to
`emitted_method_contains`; they must not join and re-split them or canonicalize either argument
again. This is equivalent to the old predicates because their old `rsplit_once("::")` occurred only
while scanning each candidate `Function::name`; index construction now performs that same split.

All six audited membership predicates are in scope:

1. `lower_inst/method_resolution.rs::class_method_already_emitted`;
2. `lower_inst/objects/interface_layout.rs::emitted_instance_method_keys`;
3. `lower_inst/objects/interface_layout.rs::class_method_already_emitted`;
4. `lower_inst/descriptor_metadata.rs::class_method_body_exists`;
5. `lower_inst/iterators.rs::class_method_body_exists`; and
6. `lower_inst/builtins/arrays/callback_targets.rs::instance_method_already_emitted`.

Small existing facades may remain to preserve Rust module visibility and imports, but none may scan
`module.class_methods` after this change.

## Determinism and exclusions

Neither `HashSet` may be iterated to choose output order. The borrowed instance view is used only
for `contains` while the existing deterministic `interface_info.method_order` traversal remains
authoritative. Method bodies continue to be emitted by the existing source-order vectors.

The following scans are explicitly excluded because they inspect additional flags, build richer
metadata, or control emission order: generator detection, eval/runtime method inventories,
reflection metadata, intrinsic-wrapper construction, and ordinary method-body emission loops.

## Verification

A differential unit test builds a small `Module` containing instance and static methods, mixed
method case, a missing delimiter, a class containing `::`, and raw class prefixes that differ only
by a leading backslash. It compares index membership with the old scan semantics over a query
matrix, proving suffix splitting, method-only canonicalization, instance/static separation, and raw
class matching.

Focused end-to-end tests cover direct instance and static calls, inheritance, interface method
validation, iterator method resolution, first-class method callables, and array callback targets.
Generate the same representative assembly twice and compare bytes to ensure deterministic output.
Run warning-free library compilation, targeted formatting, assembly-comment validation for touched
codegen files, and `git diff --check`.

After those gates pass, profile the same large `--web` build. Any further cache, interface-closure,
reachability, or EIR-validation optimization is a separate evidence-driven change and requires its
own semantic audit.
