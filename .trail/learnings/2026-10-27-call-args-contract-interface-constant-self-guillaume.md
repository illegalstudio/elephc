---
title: "__call args contract, interface constant self::, Carbon blockers"
author: Guillaume
date: 2026-10-27
branch: reconcile/dirname-symfony
---

## Decided
- `__call`/`__callStatic` `$args` is forced to `array<mixed>` in the EIR signature (`eir_signature_with_php_param_contracts`, `magic_call_args_param`). The checker specializes it from typed call sites (`specialize_magic_call_signature`, seeded `Array<Never>`), but calls through a `mixed`/`object` receiver and `__elephc_magic_call()` pass the array they hold unconverted: the body read the caller's ints as cell pointers. Carbon's `day(1)->month(6)` chain answered the current date. Pre-existing since at least c352.
- Interface constant initializers now get the same lexical `self::` rewrite as class constants (`resolve_lexical_interface_constant_value`, schema/interfaces.rs): a raw `self::` was resolved against the class reading the constant (Carbon `TranslationOptions::TRANSLATE_ALL` via a trait parameter default).
- A folded `require $m->hasReturnType() ? …` also drops the `$m = new ReflectionMethod(...)` binding when nothing else reads it (`drop_consumed_reflection_bindings`); otherwise the file-scope-effect check defers files reached only from function bodies.
- Builtin date classes emit `__serialize`/`__unserialize` like the magic hooks (`push_constructor_and_interface_methods`): the runtime and `parent::` reach them without naming the builtin.

## Tried and ruled out
- Editing `magic_method_param_keeps_eir_contract` for `__call`: it only covers UNTYPED params; a declared `array $args` never goes through it.

## Watch out
- A bisect worktree built into the same CARGO_TARGET_DIR left HEAD's `elephc-builtin-contract` in place: new catalog constants read as undefined. Touch the crate and rebuild, or give the bisect its own target dir.
- `class_exists()` of an interface is false in php: a fixture that relies on it taking the interface branch is wrong.
