---
title: "mysqli_character_set_name() — internals"
description: "Compiler internals for mysqli_character_set_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 101
---

## `mysqli_character_set_name()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:211](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L211) (`mysqli_character_set_name`)
- **Function symbol**: `mysqli_character_set_name()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_character_set_name(mixed $mysql): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_character_set_name()`](../../../php/builtins/database/mysqli_character_set_name.md)
