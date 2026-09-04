---
title: "mysqli_set_charset() — internals"
description: "Compiler internals for mysqli_set_charset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 155
---

## `mysqli_set_charset()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:191](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L191) (`mysqli_set_charset`)
- **Function symbol**: `mysqli_set_charset()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_set_charset(mixed $mysql, string $charset): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_set_charset()`](../../../php/builtins/database/mysqli_set_charset.md)
