---
title: "getimagesizefromstring() — internals"
description: "Compiler internals for getimagesizefromstring(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 452
---

## `getimagesizefromstring()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:3856](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L3856) (`getimagesizefromstring`)
- **Function symbol**: `getimagesizefromstring()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function getimagesizefromstring(string $data): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `getimagesizefromstring()`](../../../php/builtins/image/getimagesizefromstring.md)
