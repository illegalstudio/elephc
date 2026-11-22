---
title: "Checker refusals of constructs php binds at run time (Demo deferrals, bead .92.6)"
author: Guillaume
date: 2026-11-22
branch: bead-deferred
touches:
  - src/types/checker/functions/call_validation.rs
  - src/types/checker/inference/expr/effects.rs
  - src/types/checker/stmt_check/assignments/properties.rs
  - src/generics.rs
  - src/ir_lower/expr/call_arg_coercion.rs
  - tests/codegen/runtime_checked_refusals.rs
---

## Decided
- A `mixed` argument at a BUILTIN class method's `object` parameter (`ReflectionClass::isUninitializedLazyObject`, `isInstance`) was refused: internal classes are not "coercive", so `check_known_callable_call_with_options` took the strict `require_compatible_arg_type` branch, which never asks `classify_param_binding` for `ParamBinding::ObjectGuard`. It now does, and lowering already guards it. php names no `called in` clause for an internal method; `call_description_is_internal_method` drops it.
- `use (&$error)` of a name the enclosing body never bound CREATES the variable: the checker kept reading it as an undefined (null) local, so `throw $error` / `new RuntimeException($error)` after the closure ran were refused ("throw requires an object value", "Exception::__construct parameter $message expects string, got null"; Doctrine `Instantiator`, symfony/finder `SplFileInfo`, symfony/runtime `FrankenPhpWorkerRunner`). The post-statement hook that widens a boxed null capture to `Mixed` now also covers a name absent from the env.
- `object|string $o` after `if (is_object($o)) { $o = $o::class; }` keeps the declared union (a store inside a guarded region is measured against the binding), so `$this->r[$o] ??= ...` was refused. `property_write_key_type` now boxes an `Object` member into the `Mixed` key like the scalar members: php only throws if an object reaches the key.
- A closure literal's `self` return type at a PHPDoc `@template` call (`Factory::create($m, static function (int $c): self {...})` inside a static method) bound `T = self` and resolved it outside the class ("Cannot use 'self' as a type outside of a class", DBAL `IBMDB2\Exception\*`). `infer_bindings_with_args` takes the call's lexical class and substitutes relative types in the closure's declared types.
- An interface-typed object at a union of unrelated interfaces (`Conf|ConfExt`, symfony `ConfigDumpReferenceCommand::getExtensionDocUrl`) is admitted by `object_argument_may_narrow_to`, and `guard_object_class_union_param` (lowering) checks every member with `instanceof` and throws php's `TypeError`. It must run BEFORE the `param_ty == Mixed` early boxing return of `coerce_scalar_arg_to_param_storage` (a union's codegen repr is `Mixed`) and must not box its operand: boxing an owned temporary and releasing the probe freed the object.

## Watch out
- The union TypeError names the argument's STATIC class (`Ext given`); php names the runtime class (`B given`). Same for `isUninitializedLazyObject`: php says `must be of type Pt` (the receiver's class), elephc says `object`.
- `ReflectionProperty::setRawValueWithoutLazyInitialization(5, $v)` still does not throw (no call-site guard reaches the prelude's delegating method); only the compile refusal is fixed. A `?object` user parameter handed an int is silently accepted too (php: TypeError); `coerce_scalar_arg_to_param_storage` only guards `Object(class)`, not `Object|null` with a gradual source.
- A local `$r[$object]` with an `object|string` key (not a property) compiles but does not throw "Cannot access offset of type stdClass on array" when an object arrives.
- Standalone vendor probes (`require vendor/autoload.php`, `new \Vendor\Class`) do not pull the class into a `--check`; the Demo deferrals only show with the real console + `opcache.preload` build.
