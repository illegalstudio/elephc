---
title: Property references from untyped receivers and through reference elements
author: Guillaume
date: 2026-10-26
branch: reconcile/dirname-symfony
---

## Decided
- `__rt_mixed_array_get_for_write` follows array reference markers (tags INVOKER_ARG / ARRAY_GLOBAL / ARRAY_LOCAL ref cells) instead of detaching them: a marker over a boxed referent (`[m+16] == 7`) returns the referenced box (incref'd), so `$h = [&$v]; $h[0]['y'] = 2` reaches `$v`. Other referents keep the clone path. Both arches; x86 verified under the Docker linux/amd64 image.
- An untyped parameter receiver is UNKNOWN for references even when the checker narrowed it: the checker keeps the first call site's class in the signature (`object: A` after `lit(new A); lit(new B)`), lowering stores it as Mixed. `receiver_is_untyped_parameter` (stmt_check/assignments/locals.rs) detects it from `FunctionSig::declared_params`.
- Unknown receiver + literal name: `widen_property_for_unknown_receiver_reference` promotes and widens the name on every declaring class (visible, non-hooked). Lowering (`lower_ref_assign_property`) uses `LoadPropRefCellChecked` for Mixed receivers with the payload all promoted cells share, so a mismatching class raises an Error instead of a box written into a raw int cell.
- Unknown receiver + runtime name (`&$object->$name`, PropertyAccessor:434/451): every visible non-readonly declared property is widened/promoted; codegen `lower_mixed_dynamic_prop_ref_cell` = class-id ladder then name ladder over Mixed reference slots. Miss = catchable Error (never a zero cell).
## Tried and ruled out
- Taking the lowering local type from `ctx.local_types`: it is lowering's view (Mixed), not the checker's.
- Emitting the checked op with IrType::I64 and a non-int php type: EIR validator PhpTypeMismatch; use `ctx.emit_value`.
## Watch out
- stdClass / undeclared-name references (nette Arrays:337) are not supported: per-instance hash refs need a hash-entry cell (bead "References into per-instance property hashes").
- Widened typed properties lose php's TypeError on wrong-typed writes through the reference, and box every access.
