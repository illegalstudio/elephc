---
title: "tidy_parse_string() - internals"
description: "Compiler internals for tidy_parse_string(): lowering path, type checks, and runtime helpers."
---

## `tidy_parse_string()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_tidy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_tidy.rs)
- **Lowering**: [`src/tidy_prelude/build/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/tidy_prelude/build/functions.rs) (`tidy_parse_string`)
- **Function symbol**: `tidy_parse_string()`


### Lowering notes

- Implemented by the compiler-injected tidy prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function tidy_parse_string(string $string, array|string|null $config = null, ?string $encoding = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/tidy/tidy_parse_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tidy/tidy_parse_string.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `tidy_parse_string()`](../../../php/builtins/tidy/tidy_parse_string.md)
