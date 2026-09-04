---
title: "session_encode() — internals"
description: "Compiler internals for session_encode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 857
---

## `session_encode()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3046](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3046) (`session_encode`)
- **Function symbol**: `session_encode()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_encode(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_encode()`](../../../php/builtins/web/session_encode.md)
