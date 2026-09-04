---
title: "mysqli_real_escape_string() — internals"
description: "Compiler internals for mysqli_real_escape_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 148
---

## `mysqli_real_escape_string()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:230](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L230) (`mysqli_real_escape_string`)
- **Function symbol**: `mysqli_real_escape_string()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_real_escape_string(mixed $mysql, string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_real_escape_string()`](../../../php/builtins/database/mysqli_real_escape_string.md)
