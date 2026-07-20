---
title: "__elephc_ptr_read_string() — internals"
description: "Compiler internals for __elephc_ptr_read_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 470
---

## `__elephc_ptr_read_string()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/__elephc_ptr_read_string.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/__elephc_ptr_read_string.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:129](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L129) (`lower_ptr_read_string`)
- **Function symbol**: `lower_ptr_read_string()`


### Lowering notes

- Lowers `ptr_read_string(pointer, length)` by copying raw bytes into an owned PHP string.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ptr_read_string`

## Signature summary

```php
function __elephc_ptr_read_string(mixed $pointer, mixed $length): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
