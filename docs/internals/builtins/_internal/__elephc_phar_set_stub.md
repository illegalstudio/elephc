---
title: "__elephc_phar_set_stub() — internals"
description: "Compiler internals for __elephc_phar_set_stub(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 451
---

## `__elephc_phar_set_stub()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_set_stub.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_set_stub.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3896](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3896) (`lower_elephc_phar_set_stub`)
- **Function symbol**: `lower_elephc_phar_set_stub()`


### Lowering notes

- Lowers `__elephc_phar_set_stub()` into the stub-write bridge call.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_set_stub(string $filename, string $stub): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
