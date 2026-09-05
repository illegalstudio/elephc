---
title: "timezone_abbreviations_list() — internals"
description: "Compiler internals for timezone_abbreviations_list(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 243
---

## `timezone_abbreviations_list()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/tz_prelude.rs`:73](https://github.com/illegalstudio/elephc/blob/main/src/tz_prelude.rs#L73) (`timezone_abbreviations_list`)
- **Function symbol**: `timezone_abbreviations_list()`


### Lowering notes

- Implemented by the compiler-injected tz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function timezone_abbreviations_list(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `timezone_abbreviations_list()`](../../../php/builtins/date/timezone_abbreviations_list.md)
