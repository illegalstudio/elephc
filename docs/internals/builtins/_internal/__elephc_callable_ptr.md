---
title: "__elephc_callable_ptr() — internals"
description: "Compiler internals for __elephc_callable_ptr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 438
---

## `__elephc_callable_ptr()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/elephc_callable_ptr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/elephc_callable_ptr.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:111](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L111) (`lower_elephc_callable_ptr`)
- **Function symbol**: `lower_elephc_callable_ptr()`


### Lowering notes

- Lowers `__elephc_callable_ptr($cb)` — the runtime value of a closure or
- first-class callable already IS the raw pointer to its 64-byte descriptor, so
- this is a bare identity load into the pointer result register.
- Dynamic PHP callable forms are normalized into the same descriptor ABI: strings
- select a static function/method descriptor, arrays select a static or receiver-
- bound method descriptor, invokable objects bind `__invoke`, and boxed callable
- descriptors preserve their existing payload.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_callable_ptr(mixed $value): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
