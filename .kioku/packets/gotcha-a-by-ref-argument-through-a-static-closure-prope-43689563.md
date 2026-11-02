---
id: gotcha-a-by-ref-argument-through-a-static-closure-prope-43689563
type: gotcha
title: "A by-ref argument through a static Closure property has no tracked signature"
description: "Symfony AbstractAdapter::commit reports Undefined variable: $expiredIds because a static property holding a closure has no tracked callable signature, and defining the variable would miscompile"
created: 2026-09-17
sources:
  - path: src/types/checker/inference/ops.rs
    blob: 8c6d6292056d9e39727e4fb07a1c05cc08b66467
  - path: src/ir_lower/expr/descriptor_calls.rs
    blob: fbbd21a95a66d6f287ced1af4b0dc00778b82c43
superseded_by: "bug_fix-a-static-property-s-closure-signature-was-record-62666829"
superseded_why: "Fixed: resolve_expr_callable_sig gained the StaticPropertyAccess arm the recording side was always written for"
---

# A by-ref argument through a static Closure property has no tracked signature

## Fact

`AbstractAdapter::commit()` calls

    $byLifetime = (self::$mergeByLifetime)($this->deferred, $this->namespace, $expiredIds, ...);
    if ($expiredIds) { ... $this->doDelete($expiredIds); }

and elephc reports `Undefined variable: $expiredIds` twice. `self::$mergeByLifetime` is a static
property assigned a closure literal (through `Closure::bind`) whose third parameter is `&$expiredIds`.

Reduced to 20 lines: it reproduces even WITHOUT `Closure::bind` — a plain
`self::$merge ??= static function (array $d, &$e) {...}` then `(self::$merge)($d, $ids)` fails the
same way. So the gap is that a STATIC PROPERTY holding a closure has no tracked callable
signature; `callable_sigs` covers locals only.

`infer_expr_call_type` sees the callee type `?\Closure` = `Union([Callable, Void])`, takes its
`Mixed | Union(_)` arm, and infers each argument — which is where the undefined read is reported.
That arm also does NOT call `record_unresolved_callee_argument_aliases`, unlike the sibling `Str`
arm; that looks like a second, separate defect.

DO NOT "fix" this by defining the bare-variable argument the way the MethodCall site does. There
the by-ref shape is genuinely unknowable; here it is written in the source. And codegen's
unresolved-callee path (`lower_untyped_descriptor_invoker_arg_container` →
`emit_callable_descriptor_invoke`) passes arguments BY VALUE in a container, so a defined-but-unwired
`$expiredIds` would stay empty and Symfony's cache would silently stop deleting expired entries.
`commit()` runs on every request.

The real fix is to track the signature: record `Class::$prop -> FunctionSig` when a static property
is assigned a closure literal (agreement required across assignments, `Closure::bind` unwrapped),
consult it in `infer_expr_call_type` and in `inference/expr/effects.rs` so
`prepare_by_ref_variable_storage` runs, and resolve the callee in ir_lower so the by-ref ABI is used.

## Why

Defining the variable instead would compile a silent miscompile, not a fix
