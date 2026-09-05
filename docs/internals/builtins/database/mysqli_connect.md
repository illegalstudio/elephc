---
title: "mysqli_connect() — internals"
description: "Compiler internals for mysqli_connect(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 104
---

## `mysqli_connect()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:69](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L69) (`mysqli_connect`)
- **Function symbol**: `mysqli_connect()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_connect(?string $hostname = null, ?string $username = null, ?string $password = null, ?string $database = null, ?int $port = null, ?string $socket = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–6 arguments (6 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_connect()`](../../../php/builtins/database/mysqli_connect.md)
