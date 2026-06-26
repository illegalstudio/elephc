---
title: "__elephc_phar_list_entries() — internals"
description: "Compiler internals for __elephc_phar_list_entries(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 414
---

## `__elephc_phar_list_entries()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3634](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3634) (`lower_elephc_phar_list_entries`)
- **Function symbol**: `lower_elephc_phar_list_entries()`


### Lowering notes

- Internal helper used by the built-in Phar / PharData support to enumerate archive entries.
- Calls the native PHAR listing bridge and returns the entries as an array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_list_entries(mixed $filename): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
