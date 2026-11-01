---
id: decision-php-symbol-key-equality-is-eq-ignore-ascii-case--0bab531b
type: decision
title: "php_symbol_key equality IS eq_ignore_ascii_case, and the tree pays a String per candidate for it"
description: "169 sites compare two lowercased keys; inside a whole-table scan that is one heap allocation per class per lookup, and the allocation-free form is the same predicate"
created: 2026-09-20
sources:
  - path: src/ir_lower/expr/array_access_types.rs
    blob: 4fa121578d851fc542722454ef8e91e64428081d
  - path: src/ir_lower/expr/callable_resolution.rs
    blob: e3690b305f2ef71c86897db77e91277cac6c51be
  - path: src/ir_lower/reflection.rs
    blob: e7024b1680fa3066e05f21c5e71fff98dd75a266
---

# php_symbol_key equality IS eq_ignore_ascii_case, and the tree pays a String per candidate for it

## Fact

`php_symbol_key(name)` is nothing but `name.to_ascii_lowercase()` (src/names.rs:213). So

    php_symbol_key(a) == php_symbol_key(b)    <=>   a.eq_ignore_ascii_case(b)
    php_symbol_key(a) == "lowercase-literal"  <=>   a.eq_ignore_ascii_case("lowercase-literal")

are the SAME predicate — the ASCII fold is byte-wise. The first form allocates a fresh String
for every candidate examined; the second allocates nothing. There were 169 comparison sites of
this shape in src/.

This matters because the expensive ones sit inside a scan of a whole module table, so the cost
is one heap allocation per class (or per function, or per interface) per lookup:

  src/ir_lower/expr/callable_resolution.rs  lookup_folded_name, run against ctx.classes,
                                            ctx.functions AND ctx.extern_functions per
                                            string-callable site
  src/ir_lower/expr/array_access_types.rs   interface_extends_interface_for_ir recomputed the
                                            ancestor's key INSIDE the closure, so a recursive
                                            diamond walk allocated it once per parent per level
  src/ir_lower/expr/reflection_new_instance.rs  resolve_known_class_name / _function_name
  src/ir_lower/expr/eval_barriers.rs        aot_class_exists_for_eval_probe and the function
                                            signature probe
  src/ir_lower/reflection.rs                canonical_builtin_reflection_class_name re-folded a
                                            15-entry static table on every call

Prefer this over indexing the table by key when the scan is a `.find`: an index changes WHICH
candidate wins when two names differ only in case, and `eq_ignore_ascii_case` does not. Use an
index (the `class_key_index` pattern in src/codegen/shared_state.rs) only where that tie-break
is already settled.

## Why

Four advisor models independently ranked whole-module name scans as the remaining compile-time lever; this is the one transform among them that cannot change behaviour at all.
