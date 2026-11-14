---
title: "preg_grep() - internals"
description: "Compiler internals for preg_grep(): lowering path, type checks, and runtime helpers."
---

## `preg_grep()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pcre_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/pcre_prelude.rs) (`preg_grep`)
- **Function symbol**: `preg_grep()`


### Lowering notes

- Implemented by the compiler-injected PCRE prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function preg_grep(string $pattern, array $array, int $flags = 0): array|false
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_grep.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_grep.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `preg_grep()`](../../../php/builtins/regex/preg_grep.md)
