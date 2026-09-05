---
title: "session_decode() — internals"
description: "Compiler internals for session_decode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 855
---

## `session_decode()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3064](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3064) (`session_decode`)
- **Function symbol**: `session_decode()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_decode(string $data): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_decode()`](../../../php/builtins/web/session_decode.md)
