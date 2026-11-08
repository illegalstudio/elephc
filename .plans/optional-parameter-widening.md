# Optional-parameter widening for overrides and interface implementations

- [x] Search existing issues and Pull Requests (#1935 is the related design discussion).
- [x] Draft the issue text (below; not opened).
- [x] Checker: accept trailing optional parameters on instance overrides, interface
      implementations and interface redeclarations (`ParameterWidening::TrailingOptional`).
- [x] Checker: retire the inherited instance-vtable slot under a generated key and give the
      widened method a new slot (`ClassInfo::narrowed_vtable_slots`).
- [x] Reachability: keep and compact retired slots with their method; teach the debug
      slot-alignment assertion about them.
- [x] EIR: lower one adapter per (implementing class, retired slot) with the ancestor's ABI.
- [x] Codegen: publish the adapter in the physical and source-visible class vtables.
- [x] Codegen tests (narrow and wide receivers, named arguments, constant and expression
      defaults, by-reference parameters, `func_num_args()`/`func_get_args()`, collecting
      parents, interface redeclaration and implementation) and error tests.
- [x] Docs (`docs/php/classes.md`, `docs/php/types.md`, `docs/internals/the-type-checker.md`)
      and the `examples/override-widening` program.
- [ ] Follow-ups (separate changes): static-method widening, appended variadics, making an
      inherited required parameter optional, `mixed`-receiver calls across differing arities.

## Issue draft

**Title:** Overrides and interface implementations cannot append optional parameters
(`Cannot change parameter count when overriding method`)

PHP lets a method that overrides a parent method, implements an interface method, or
redeclares a parent interface's method append trailing optional parameters: every call valid
against the narrower declaration stays valid (LSP). elephc rejects all three:

```php
<?php
interface PsrDispatcher { public function dispatch(object $event); }
interface SfDispatcher extends PsrDispatcher {
    public function dispatch(object $event, ?string $eventName = null): object;
}
// error: Cannot change parameter count when redeclaring interface method: SfDispatcher::dispatch

interface PsrContainer { public function get(string $id); }
class Container implements PsrContainer { public function get(string $id, int $invalidBehavior = 1) {} }
// error: Cannot change parameter count when implementing interface method: Container::get

abstract class A { abstract public function f(): string; }
class B extends A { public function f(bool $x = true): string { return $x ? "t" : "f"; } }
// error: Cannot change parameter count when overriding method: B::f
```

These are the exact shapes of psr/event-dispatcher vs symfony/event-dispatcher-contracts,
psr/container vs symfony/dependency-injection, and symfony/string's `folded()`, so none of
those packages compiles.

The checker refuses because of the call ABI, not the PHP rule: a call through the parent or
interface type passes the parent's argument count, so the override's extra parameters must
receive their defaults at run time. Final enums already get this through the EIR interface
adapter (`MethodAbiPlan::OptionalDefaults`, gated by `allow_optional_widening`); classes do
not, because a class vtable slot is shared by every member of the override family.

#1935 discusses the general "one call ABI per override family" design and asks (question C)
how an override that adds defaults should be handled. This proposal covers only the
arity side of it, with per-slot adapters (#1935's option 4), and leaves representation
unification to #1935.

**Proposal.** When an instance override appends optional parameters, the checker retires the
inherited vtable slot instead of reusing it: the slot keeps its index under a generated key,
and the method key gets a new, wider slot. Codegen fills the retired slot with an EIR adapter
that has the ancestor's ABI and forwards exactly the arguments it received to the override,
so ordinary call lowering supplies the defaults and `func_num_args()` counts what the caller
passed. Interface implementations reuse the existing per-class interface adapters.

**Still rejected:** removing a parameter, appending a required one, making an optional one
required, changing by-reference mode, narrowing an inherited untyped parameter; and, for now,
widening a static method, appending a variadic, and making a required parameter optional.

## Design

### Mechanism

Per retired slot, an EIR adapter (C++-style thunk, #1935's option 4):

1. `schema/validation.rs`: `validate_signature_compatibility` takes a `ParameterWidening`
   (`Exact`, `TrailingOptional`, `CallShape`) instead of `allow_optional_widening: bool`.
   `TrailingOptional` accepts a child that appends parameters, all with defaults, to a
   non-variadic parent, then runs the unchanged exact checks over the inherited prefix
   (`SourceVisibleShape::truncated`). The generated `func_args` collector check is skipped
   for such a widening, because the adapter calls the child through its own signature.
   `CallShape` is the former enum-only path. Static and magic (`__*`) methods stay `Exact`.
2. `schema/classes/narrowed_slots.rs`: after a widening instance override validates, the
   inherited slot moves from the method key to `"<method>#abi<inherited param count>"` in
   `vtable_methods` / `vtable_slots`, recorded in `ClassInfo::narrowed_vtable_slots`
   (method key + signature snapshot). The method key then gets a fresh slot through the
   ordinary allocation. `#` cannot appear in a PHP identifier. Successive widenings retire
   one slot each; subclasses inherit the map.
3. `optimize/reachability/reconcile.rs`: a retired key is kept exactly when its method is,
   the map is filtered with the vector before compaction, and the debug assertion
   `assert_inherited_vtable_slots_aligned` accepts a moved method key when the parent's
   slot number survives under a retired key of the same method.
4. `ir_lower/function/narrowed_vtable_adapters.rs`: for every class and retired slot with an
   implementation, one adapter function named `_vtnarrow_<owner>_<key>` (deduplicated per
   owner) is lowered by reusing `lower_interface_adapter`: parameters = the ancestor's
   signature, body = `return $this->method(<those parameters>)` on a receiver typed as the
   owner, so the call dispatches through the owner's wider slot (deeper overrides still win)
   and the default expressions are lowered by the ordinary call path. When the ancestor's
   signature carries generated `func_args` slots, a second `_vtnarrow_src_…` adapter with
   the source-visible signature serves the source vtable. The caller signature is the
   ancestor's final signature (`ClassInfo::narrowed_slot_signature`), because untyped
   parameter storage is refined after the class schema is built; the snapshot is only a
   fallback.
5. `codegen_support/runtime/data/user.rs`: the physical (`_class_vtable_<id>`) and source
   (`_class_source_vtable_<id>`) tables publish `_fn_<adapter>` for a retired key, or `0`
   for an abstract implementation, as for the method's own slot.
   `runtime_metadata/classes.rs` maps a retired key to its method for the dynamic-instanceof
   symbol check.

No assembly is emitted by hand: the adapter is EIR, so every target (aarch64 macOS/Linux/iOS,
x86_64 Linux) uses its ordinary lowering. Direct calls, final receivers and `parent::` calls
are unchanged. Interface calls already go through per-(class, interface) entries planned by
`plan_method_abi`, which yields `OptionalDefaults` for an appended tail.

### Why

- Callers keep their static ABI: nothing changes at call sites, and the cost (one extra call)
  is paid only by calls through the narrower type into a widened override.
- Defaults (constants, expressions, `static`/`self`) are evaluated by the existing call
  lowering in the override's scope, not re-implemented in a thunk.
- `func_num_args()` stays correct for free: the adapter forwards only what it received,
  including surplus arguments collected by a `func_num_args()`-using parent.
- It reuses a mechanism already proven for enums and interfaces.

### Alternatives considered

- **Callee-side default materialization** (pass an actual-count or "missing" sentinel and let
  the override fill its defaults): changes the ABI of every method in every family and every
  call site, and duplicates `func_args`' hidden-count machinery.
- **Widest ABI in the shared slot**: callers through the parent cannot know a subclass's
  defaults, so it degenerates into callee-side materialization.
- **Single slot, adapter for every caller arity**: the slot would need one ABI per static
  receiver type; a second slot per widening is the standard C++ covariant-thunk answer.
- **Checker-only relaxation** (as #1806 does for added defaults): miscompiles, since the
  override would read the missing arguments from uninitialized registers.
- **#1935's boxed family ABI**: solves representation mismatches too, but is a much larger
  ABI change still under discussion; this change is orthogonal and composes with it.

### Affected passes

Type checker (signature validation, class vtable construction), reachability pruning, EIR
lowering (adapter functions), runtime data emission (class vtables). Parser, resolver,
name resolver and optimizer are not involved.

### Test plan

- `tests/codegen/oop/optional_parameter_widening.rs`: the reported shapes called through the
  narrow and the wide type; chains of widenings with an inheriting grandchild; named
  arguments; constant and expression defaults; `func_num_args()`/`func_get_args()` through
  both types; surplus arguments through a collecting parent; abstract `static` return;
  interface redeclaration and implementation with an overriding subclass; by-reference
  parameters; protected methods reached from the parent's body. Each fails on main (the
  checker rejects the program) and matches PHP 8.5 (`ELEPHC_PHP_CHECK=1`).
- `tests/error_tests/misc/optional_parameter_widening.rs`: removed parameter, appended
  required parameter, optional-to-required, by-reference changes in both directions,
  narrowed untyped parameter, interface variants, static override.

### Known gaps (follow-ups)

- Static methods (`static_vtable_*`, late static binding) keep the exact rule.
- Appended variadics and required-to-optional changes stay rejected; PHP accepts both.
- A `mixed`/untyped receiver dispatches through a candidate ladder filtered by operand count,
  so a widened class is skipped and the call reports a member call on `null`. This already
  happens on main for unrelated classes whose same-named methods differ in arity.
- Pre-existing and independent of widening: a call through a non-final receiver fills an
  omitted optional argument with the STATIC receiver's default, so an override that changes an
  inherited default (`get($id, $b = 1)` overridden as `get($id, $b = 2)`, called as
  `$p->get("x")` through the parent type) sees the parent's default; PHP uses the override's.
  Calls through a retired slot are not affected (the adapter uses the override's signature),
  but calls through an intermediate wide type are. #1935 tracks inherited-default changes.
- Returning a `mixed` value from a function declared `: object` or `: SomeClass` is a
  compile error (`return type expects object, got mixed`) independently of this change; the
  PSR-style `function viaPsr(PsrDispatcher $d, object $e): object { return $d->dispatch($e); }`
  therefore still needs that separate fix.
