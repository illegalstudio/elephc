---
title: "__elephc_phar_get_signature_type() — internals"
description: "Compiler internals for __elephc_phar_get_signature_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 435
---

## `__elephc_phar_get_signature_type()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_get_signature_type.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_get_signature_type.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4206](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4206) (`lower_elephc_phar_get_signature_type`)
- **Function symbol**: `lower_elephc_phar_get_signature_type()`


### Lowering notes

- Lowers `__elephc_phar_get_signature_type(path)` into the signature-type read bridge.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_get_signature_type(string $path): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
