---
title: "mysqli_change_user() - internals"
description: "Compiler internals for mysqli_change_user(): lowering path, type checks, and runtime helpers."
---

## `mysqli_change_user()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs) (`mysqli_change_user`)
- **Function symbol**: `mysqli_change_user()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_change_user(mixed $mysql, string $username, string $password, ?string $database): bool
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_change_user()`](../../../php/builtins/database/mysqli_change_user.md)
