---
title: "__elephc_phar_list_entries() — internals"
description: "Compiler internals for __elephc_phar_list_entries(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 461
---

## `__elephc_phar_list_entries()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_list_entries.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_list_entries.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4273](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4273) (`lower_elephc_phar_list_entries`)
- **Function symbol**: `lower_elephc_phar_list_entries()`


### Lowering notes

- Internal helper used by the built-in Phar / PharData support to enumerate archive entries.
- Calls the native PHAR listing bridge and returns the entries as an array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_list_entries(string $filename): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
