---
title: "mysqli_get_server_info() — internals"
description: "Compiler internals for mysqli_get_server_info(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 133
---

## `mysqli_get_server_info()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:435](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L435) (`mysqli_get_server_info`)
- **Function symbol**: `mysqli_get_server_info()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_get_server_info(mixed $mysql): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_get_server_info()`](../../../php/builtins/database/mysqli_get_server_info.md)
