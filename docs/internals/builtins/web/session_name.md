---
title: "session_name() — internals"
description: "Compiler internals for session_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 862
---

## `session_name()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:2979](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L2979) (`session_name`)
- **Function symbol**: `session_name()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_name(?string $name = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_name()`](../../../php/builtins/web/session_name.md)
