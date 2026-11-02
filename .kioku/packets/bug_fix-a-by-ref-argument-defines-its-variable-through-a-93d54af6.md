---
id: bug_fix-a-by-ref-argument-defines-its-variable-through-a-93d54af6
type: bug_fix
title: "A by-ref argument defines its variable through an X|false receiver - the callable resolver was masking the real signature"
description: "PdoAdapter bindParam reported Undefined variable because resolve_first_class_callable_sig answered before the class schema with a one-parameter non-ref signature"
created: 2026-09-17
sources:
  - path: src/types/checker/inference/expr/effects.rs
    blob: c38dba80b693b1dbbf422f13afc0f178c1adce6d
supersedes: "gotcha-a-by-reference-argument-does-not-define-its-vari-f01f8d26"
---

# A by-ref argument defines its variable through an X|false receiver - the callable resolver was masking the real signature

## Fact

A by-reference argument now DEFINES its variable through an `X|false` receiver. The union was never
the cause.

    function f(PDO $conn, string $sql): string {
        $stmt = $conn->prepare($sql);     // PDOStatement|false
        $stmt->bindParam(1, $id);         // php: defines $id
        return var_export($id, true);     // elephc: Undefined variable: $id
    }

The earlier reading blamed `instance_call_effect_signature` resolving only from `PhpType::Object`,
and a partial fix there was reverted for not moving the preload count. Tracing the call site shows
why it could not have worked:

    [byref-site] ->bindParam() recv=Union([Object("PDOStatement"), Bool]) sig=Some([false])

Two facts the earlier note had wrong. `PDOStatement` IS in `self.classes` (`in_classes=true`), so
the prelude-registration theory was a dead end. And `instance_call_effect_signature` was never
reached at all: the call site tries `resolve_first_class_callable_sig` FIRST, and it answered — with
a ONE-parameter signature whose single `ref_params` entry is `false`, masking the real `bindParam`
(five parameters, `[false, true, false, false, false]`).

The class schema is the authority for a method call, so it is asked first now; the callable resolver
stays the fallback for receivers no schema can answer. `union_single_object_class` also had to be
matched on the DECLARED type rather than `codegen_repr()`, which collapses every union to `Mixed` —
a union arm placed after that call can never match, and that is worth remembering for any other
union-shaped check.

NOT PINNED BY A TEST, deliberately. The only reproducer needs a prelude class with a by-ref
parameter typed `mixed`, which accepts the freshly created null. `PDOStatement::bindParam` is one
and the default test build has no PDO feature; `Imagick::subImageMatch` is always on but declares
its by-ref parameters `array`/`float`, so after the fix it correctly reports
`parameter $offset expects Array(Mixed), got Void` instead. A userland class does not reproduce at
all — `resolve_first_class_callable_sig` answers correctly for those, which is exactly why the
masking went unnoticed. Adding a `mixed`-typed by-ref method to an always-on prelude class, or
running `error_tests` with a pdo feature, would close this.
