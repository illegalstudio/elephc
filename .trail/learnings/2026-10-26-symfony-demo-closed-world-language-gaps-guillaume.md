---
title: "Symfony Demo closed-world: language gaps fixed, traps in the SPL/lazy/reference model"
author: Guillaume
date: 2026-10-26
branch: reconcile/dirname-symfony
touches:
  - elephc.toml
---

## Decided
- Element/property references: every source reaches an element or append target through a hidden
  `$t = &source` local (parser `complex_ref_store` + `bound_through_hidden_local`); a property target
  fed by an element binds the reverse way (`$t=&$a[k]; $o->p=$t; $a[k]=&$o->p`) so both share the
  property's cell. An array marker over a property needs Mixed storage: the checker widens the
  property (`widen_property_for_element_reference`), untyped AND scalar-typed (typed markers are not
  honored by every reader, e.g. the array_splice prelude; the cost is no TypeError for a wrong-typed
  write through the reference).
- By-ref promoted constructor params over instance properties are bound natively
  (`promote_by_ref_property_args`), so `ast_usage` no longer records them as deferral places;
  Symfony's OrderedHashMap/OrderedHashMapIterator stay compiled with php's output.
- ArrayObject is one native `$storage` array (php key semantics, O(1), no leak); new SPL prelude
  methods must be added to BOTH `spl_support.rs` (emitted bodies) and `spl_metadata.rs` (required
  for dynamic dispatch) or the vtable slot is null and `$o[] = x` jumps to address 0.
- Late PDO/mysqli prelude gate after autoload; the late preludes are name-resolved separately
  (they carry namespace/use statements) and extern decls are deduplicated.
- Weak float -> int/?int params reuse `Op::ArgumentStringToInt` (float path: diagnostics, then
  fcvtzs/cvttsd2si); literal 2.5/NAN go the same way (deprecation / TypeError at run time).
- `static::C` in autoloaded files -> `constant(static::class . '::C')`; union property reads where a
  member lacks the property keep the runtime-fault tag (by-name helper).
## Tried and ruled out
- Typed (int-tagged) array markers over a typed property cell: crashes in `__elephc_array_splice_gradual`.
- Accepting properties as fsockopen error outputs without a hidden local: the builtin writes local
  slots only, the property silently kept its value.
## Watch out
- error_tests run the checker in-process: `CARGO_BIN_EXE_elephc=<frozen>` is NOT a baseline for
  them; compare with `elephc --check [--strict-locals]` on the snippet instead.
- `--web` cannot be combined with `--check`; a demo `--check` costs ~20-60 min under load.
- symfony/demo `cache:clear` sometimes leaves no container; the demo ships its own elephc.toml.
- Open: runtime-named `&$obj->$name`, nested write through a reference element, `$x = &f()` of a
  param element, mysqli::ssl_set, SimpleXML, filter_var EMAIL/URL, 58 ext constants, lazy objects
  are eager.
