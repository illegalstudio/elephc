# Reflection slot reachability Implementation Plan

**Goal:** Stop `new ReflectionObject($obj)` / `new ReflectionClass(...)` from materializing metadata
that no reachable accessor can observe, so a reflected object costs what PHP charges for it instead
of the transitive closure of every type named in its signatures.

**Architecture:** The Reflection surface is lowered by its own path
(`ir_lower/reflection.rs`) which selects at CLASS granularity and then lowers *every* method of a
selected class. The whole-program declaration reachability pass
(`.plans/declaration-reachability.md`, delivered) already prunes at METHOD granularity for ordinary
classes but does not reach this surface. This plan gives the Reflection emitters a per-slot
reachability query derived from that pass, and gates eager metadata materialization on it.

**Tech Stack:** Rust. `src/ir_lower/reflection.rs`, `src/codegen/lower_inst/objects/reflection/*`,
the reachability pass in `src/optimize/`, `tests/codegen/oop/reflection*.rs`.

---

## Why this exists — the measured defect

`new ReflectionObject($obj)` exhausts the heap identically at `--heap-size` 8 MiB **and** 1 GiB
(flag verified applied: `size -m` reports `Segment __DATA: 1 536 589 824`). It is unbounded
allocation, not a sizing problem.

Minimal reproduction — the reflected class is three lines and unrelated to any framework:

```php
class TinyNone    { public function m(int $a): string { return 'x'; } }   // OK
class TinyIface   { public function m(ContainerInterface $c): void {} }   // OK
class TinyBuilder { public function m(ContainerBuilder $c): void {} }     // heap exhausted
new \ReflectionObject(new TinyBuilder());
```

Only the class naming a **method parameter type** differs. Measured ladder, one-line local classes:

| class named as a parameter type | lines | methods | result |
|---|---|---|---|
| `ParameterBag`, `Reference`, `Definition` | — | — | OK |
| `Container` | 441 | 24 | OK |
| `ContainerBuilder extends Container` | 1865 | **81** | **heap exhausted** |

The concrete parent passes; its child does not. Cost tracks the transitive metadata closure of every
type named in every signature. PHP charges **96 bytes** for the same `ReflectionObject` — it builds a
lazy handle.

**Site:** `emit_reflection_parameter_class_property`
(`src/codegen/lower_inst/objects/reflection/parameter_property_emit.rs:265`) eagerly materializes a
`ReflectionClass` into the `__class` slot that backs `ReflectionParameter::getClass()`, through
`emit_shallow_reflection_class_object`. "Shallow" excludes methods but includes attributes,
interfaces, traits, trait aliases and **parents**, so each named type drags its own chain.

## Two rejected fixes, and why

1. **Make `getClass()` lazy by storing the class name and calling `new ReflectionClass($name)` on
   first access.** Rejected: `ReflectionClass` by *name* is itself broken for classes brought in by
   the Composer autoloader — it raises `Class "…" does not exist` while `class_exists()` returns
   true for the same name. This would trade a heap explosion for a wrong answer.
2. **Gate materialization on whether `getClass()` is called.** The intent is right —
   `ReflectionParameter::getClass()` has been deprecated since PHP 8.0 and Symfony never calls it —
   but `class_method_already_emitted` cannot answer the question:
   `ir_lower/reflection.rs::lower_builtin_reflection_class_methods` lowers **every** method
   (`for method in &class_info.method_decls`) as soon as the Reflection class is referenced at all.

## Measured on the emitted assembly — what the materializer is NOT

`--emit-asm` on the reproduction (15 lines of PHP) writes a `.s` file of 179 MB / 5.26M lines,
which is "all of Symfony compiled", not an explosion. Within it:

| | measured |
|---|---|
| Reflection materializer bodies | **56** — sharing works |
| size of one body | **226 lines**, **1** `__rt_heap_alloc` |
| largest function in the program | `ClassExistenceResource::throwOnRequiredClass`, 189,003 lines — Symfony's own code |
| nesting observed | labels like `_eir__eir__eir__eir_main_reflection_materializer_N_reflection_materializer_M`, i.e. 4 levels |

So the bodies are small and shared; the runtime cost comes from the **call graph between nested
materializers**, not from a giant body.

⛔ Measurement trap, hit here: a first pass reported "a 1.2M-line materializer" by measuring
label → *next label*. A body ends at its first `ret` (226 lines); the gap contained unrelated code.
Measure label → first `ret`. A whole fix design was nearly built on that artifact.

🔴 Still unexplained: what makes that call graph unbounded for `ContainerBuilder` when a local
self-referential class with 40 methods, 120 classes x 12 methods, traits, attributes and
associative class constants all reflect fine. Every local reproduction attempt has passed.

## Design

Per-slot reachability, computed once per module, consumed by the emitters.

1. **Query.** `ReflectionSlotUsage` records, for each builtin Reflection class, which accessor
   methods the program can actually reach. Built from the same root set and fixed point as the
   declaration reachability pass, so the two cannot disagree.
2. **Roots.** A slot is reachable if any of: a lowered user body calls that accessor; the dynamic
   eval bridge is present (names resolve at runtime, so the full surface stays live); a first-class
   callable or callable string names it; `--with-*` forces the surface.
3. **Gate.** Each eager nested materialization consults the query and, when the accessor is dead,
   writes the slot's null/default instead of materializing. `__class` is the first client;
   `__declaring_class`, `__declaring_function` and the parameter type objects follow.

## Semantic invariants

1. A reachable accessor must observe exactly what it observes today. This plan may only remove
   materialization that **no** reachable accessor can read.
2. Fresh-object identity is unchanged: two constructor expressions stay non-identical by `===`.
3. When the eval bridge is present, nothing is gated — names are resolved at runtime.
4. Soundness over precision: keeping a slot that is in fact dead is allowed; dropping one that is
   observable is a blocker.

## Task checklist

- [ ] Task 1: `ReflectionSlotUsage` type + fixed point, unit-tested in isolation
- [ ] Task 2: expose it on the codegen context; no behavior change yet
- [ ] Task 3: gate `__class` in `emit_reflection_parameter_class_property`
- [ ] Task 4: extend to `__declaring_class` / `__declaring_function` / parameter type objects
- [ ] Task 5: end-to-end tests (below)
- [ ] Task 6: re-measure the Symfony boot and record the new frontier

## Acceptance criteria

1. The three-line reproduction above runs and prints, matching `php -n` byte for byte.
2. A program that **does** call `getClass()` still gets the full object, identical to `php -n`
   (this is the test that catches over-pruning).
3. A program that reaches the accessor through `eval` keeps the full surface.
4. Symfony's `App\Kernel` reflects without exhausting the heap, and `getProjectDir()` returns the
   project directory rather than `""`.
5. `codegen::oop::reflection*` suites stay at their current pass set (set comparison, not counts).

## Notes for whoever picks this up

- Each Symfony probe costs ~10 minutes to compile. Put every question in one probe.
- **Judge a compile by the artifact's timestamp, never by a filtered log.** `grep -E "^error"` does
  not match `EIR backend error:`; that cost an hour of reasoning on a stale binary here.
- Bisect by REMOVING ingredients from the real code. Eleven synthetic replicas (traits, attributes,
  cycles, 400 classes, 1440 methods, a self-referential class) all passed — they reproduced the
  shape without the mass. Removing the trait, then the `http-kernel` layer, then the whole Kernel,
  found it in three runs.
- `--heap-debug` / `--gc-stats` are useless on this defect: the exhaustion path prints and exits
  without touching the `_gc_*` counters, and `--gc-stats` only reports at normal termination.
