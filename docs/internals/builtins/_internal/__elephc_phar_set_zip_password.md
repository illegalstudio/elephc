---
title: "__elephc_phar_set_zip_password() — internals"
description: "Compiler internals for __elephc_phar_set_zip_password(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 429
---

## `__elephc_phar_set_zip_password()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4172](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4172) (`lower_elephc_phar_set_zip_password`)
- **Function symbol**: `lower_elephc_phar_set_zip_password()`


### Lowering notes

- Lowers `__elephc_phar_set_zip_password(password)` into the ZipCrypto password
- bridge that lets later reads decrypt encrypted ZIP entries.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_set_zip_password(mixed $password): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
