---
title: "session_module_name() — internals"
description: "Compiler internals for session_module_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 861
---

## `session_module_name()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3707](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3707) (`session_module_name`)
- **Function symbol**: `session_module_name()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_module_name(string $module = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_module_name()`](../../../php/builtins/web/session_module_name.md)
