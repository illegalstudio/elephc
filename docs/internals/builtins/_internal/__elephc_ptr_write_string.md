---
title: "__elephc_ptr_write_string() — internals"
description: "Compiler internals for __elephc_ptr_write_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 458
---

## `__elephc_ptr_write_string()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/__elephc_ptr_write_string.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/__elephc_ptr_write_string.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:166](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L166) (`lower_ptr_write_string`)
- **Function symbol**: `lower_ptr_write_string()`


### Lowering notes

- Lowers `ptr_write_string(pointer, string)` by copying PHP string bytes into raw memory.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_write_string`

## Signature summary

```php
function __elephc_ptr_write_string(mixed $pointer, mixed $string): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
