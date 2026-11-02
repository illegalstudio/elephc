---
title: "Laravel closed-world: class_alias, stdClass refs, conditional classes"
author: Guillaume
date: 2026-10-27
branch: reconcile/dirname-symfony
touches:
  - src/codegen_support/runtime/objects/class_alias.rs
  - src/codegen_support/runtime/objects/stdclass_prop_ref_cell.rs
  - src/stdclass_by_ref_walk.rs
  - src/autoload/lazy_init.rs
  - src/php_token_prelude.rs
  - src/ir_lower/program/spl_support.rs
  - src/codegen/lower_inst/local_loads.rs
---

## Decided
- Run-time `class_alias()` (closed world only) is a 32-byte-row table `_class_aliases` (`objects/class_alias.rs`). Every by-name lookup resolves through `__rt_class_alias_resolve`: `__rt_new_by_name`, `__rt_class_id_by_name`, dynamic `class_exists()`, and the dynamic-`new` candidate ladder (`emit_strip_leading_namespace_separator`). Without the ladder hook an alias fell to the registry fallback, which allocates WITHOUT running the constructor. The main epilogue frees the rows (`__rt_class_alias_release`) when the module calls class_alias.
- A `stdClass` property is a hash entry: `$v = &$o->{$k}` / `&$o->p` / by-ref foreach go through `__rt_stdclass_prop_ref_cell` (elem_ref_cell on obj+8, borrowed cell). The alias must take its OWN share (`bind_owned_local_ref_cell_ptr`): rebinding the property (`$o->{$k} = &$x`) drops the hash's share.
- By-ref `foreach` over a stdClass is checked AND lowered as `foreach (array_keys(get_object_vars($o)) as $k) { $v = &$o->{$k}; ... }` (`stdclass_by_ref_walk.rs`, sites published from CheckResult like bcmath operator sites).
- `$this->cache = &self::$static` binds the property slot to the static's storage: static storage is outside the heap, so incref/decref on it are no-ops.
- Closed-world conditional class files: `if (cond) { class X }` becomes the class plus a lazy-init guard throwing `Class "X" not found`; `if (!function_exists('F')) { function F; F(); }` compiles F unconditionally and hoists the call.
- `PhpToken` is an AST prelude over `token_get_all` (no pcre2: line counting via str_replace + substr_count).

## Tried and ruled out
- Making `Reflector::getAttributes()` optional for implementors: the backend then needs an interface-method symbol the class lacks. php's `Reflector` has only `__toString()`; the invented method was removed and `Reflector`-typed `getAttributes()` calls still type.

## Watch out
- SPL/Phar builtin methods are emitted only if listed in `ir_lower/program/spl_support.rs`; an unlisted method the checker accepts leaves a NULL vtable slot (silent crash on call).
- `load_ref_cell` under a narrowed Object type over a Mixed-storage cell must check tag 6 before unboxing: the flow type can be stale (a by-ref callee stored a string).
- A Closure key is boxed with tag 10, not 6: object-key paths (`$m[$closure]`, isset) must accept both.
- `constant('X')` must be lowered through a `ConstRef` expression: `fallback_expr_type` is syntactic, and the call typed a runtime `define()`'s raw int as a Mixed box.
