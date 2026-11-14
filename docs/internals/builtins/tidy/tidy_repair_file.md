---
title: "tidy_repair_file() - internals"
description: "Compiler internals for tidy_repair_file(): lowering path, type checks, and runtime helpers."
---

## `tidy_repair_file()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_tidy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_tidy.rs)
- **Lowering**: [`src/tidy_prelude/build/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/tidy_prelude/build/functions.rs) (`tidy_repair_file`)
- **Function symbol**: `tidy_repair_file()`


### Lowering notes

- Implemented by the compiler-injected tidy prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function tidy_repair_file(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): string|false
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/tidy/tidy_repair_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tidy/tidy_repair_file.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `tidy_repair_file()`](../../../php/builtins/tidy/tidy_repair_file.md)
