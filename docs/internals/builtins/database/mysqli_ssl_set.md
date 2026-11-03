---
title: "mysqli_ssl_set() - internals"
description: "Compiler internals for mysqli_ssl_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 174
---

## `mysqli_ssl_set()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:444](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L444) (`mysqli_ssl_set`)
- **Function symbol**: `mysqli_ssl_set()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_ssl_set(mixed $mysql, ?string $key, ?string $certificate, ?string $ca_certificate, ?string $ca_path, ?string $cipher_algos): bool
```

## What the type checker enforces

- **Arity**: takes exactly 6 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_ssl_set()`](../../../php/builtins/database/mysqli_ssl_set.md)
