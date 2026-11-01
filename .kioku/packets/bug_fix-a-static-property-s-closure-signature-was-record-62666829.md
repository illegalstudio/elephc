---
id: bug_fix-a-static-property-s-closure-signature-was-record-62666829
type: bug_fix
title: "A static property's closure signature was recorded and never read back"
description: "record_static_property_callable_sig wrote static_property_callable_sigs for exactly this purpose and resolve_expr_callable_sig had no StaticPropertyAccess arm, so (self::$callback)($out) never learned its by-reference parameter"
created: 2026-09-17
verified_by: "Reduction prints kept=a,b expired=old byte-identical to php -n (the expired list IS the by-ref write, so a merely-defined variable fails it); error_tests at its 99-failure baseline, identical names"
sources:
  - path: src/types/checker/callables/closures.rs
    blob: e57eec89718ac1374dacf264140592283c7056cb
  - path: src/types/checker/stmt_check/assignments/static_properties.rs
    blob: a7df2c33cda74ed60f594277d83e92c8523520e5
supersedes: "gotcha-a-by-ref-argument-through-a-static-closure-prope-43689563"
---

# A static property's closure signature was recorded and never read back

## Fact

`AbstractAdapter::commit()` reported `Undefined variable: $expiredIds` twice:

    $byLifetime = (self::$mergeByLifetime)($this->deferred, $this->namespace, $expiredIds, …);
    if ($expiredIds) { … $this->doDelete($expiredIds); }

The recording side already existed and its doc comment names this exact call. What was missing is
that `resolve_expr_callable_sig` (src/types/checker/callables/closures.rs) had no
`ExprKind::StaticPropertyAccess` arm, so `static_property_callable_sigs` was written by every
assignment and consulted by nothing.

The added arm resolves the receiver (`Named` / `self` / `static` / `parent`) to a class, maps it
through `static_property_declaring_classes` so a subclass receiver finds the parent's property,
and looks up `"{declaring}::${prop}"` — the same key the recording side writes. Resolution
failures answer `None` rather than an error: this refines a call, and the access itself raises
every real diagnostic about the receiver.

It reproduces with a plain `self::$merge ??= static function (array $d, &$e) {…}` too — the
`Closure::bind` wrapper is unwrapped on the recording side and was never the issue.

## Why

The missing link was the READ side, not the recording
