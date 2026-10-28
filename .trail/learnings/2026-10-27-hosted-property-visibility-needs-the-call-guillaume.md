---
title: "Hosted property visibility needs the call site's scope"
author: Guillaume
date: 2026-10-27
branch: fix/php-ext-followups
touches:
  - crates/elephc-zend/src/property.rs
---

## Decided
- Engine property handlers (crates/elephc-zend/src/property.rs) enforce visibility, readonly (protected(set)) and weak-mode types like php-src. Scope: EG(fake_scope) (zend_read/update/unset_property set it), else the program access's scope, else the running extension function's class.
- Program scope comes from the call site: in programs hosting extensions (OWNER_CLASS in class_infos), emit_resolved_method_call publishes the lexical class + ancestors ("Kid,DemoRecord") into the engine cell elephc_php_ext_magic_scope before any __get/__set/__isset/__unset call and clears it after. The wrapper accessor takes it first thing (elephc_php_ext_take_scope).
- Accesses run as a probe first; only protected cases return status 4 and rerun in the published scope.
## Tried and ruled out
- debug_backtrace in a prelude: flips the global backtrace gate (func_args/walk.rs), giving every function the hidden capture-all variadic; broke the variadic dispatch callback.
- Exception::getTrace: frames carry no class without that gate.
- Property hooks on wrappers: elephc refuses hook visibility/type violations at compile time and skips weak coercion.
- get_parent_class($var) in a prelude: EIR backend wants a compile-time string.
## Watch out
- A new #[no_mangle] engine static needs `/// c: hidden` or the generated header breaks extension builds.
- Assignment expression value through __set is the RHS, not PHP's coerced value (bead elephc-d0x).
