---
title: "preg_filter() - internals"
description: "Compiler internals for preg_filter(): lowering path, type checks, and runtime helpers."
---

## `preg_filter()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pcre_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/pcre_prelude.rs) (`preg_filter`)
- **Function symbol**: `preg_filter()`


### Lowering notes

- Implemented by the compiler-injected PCRE prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function preg_filter(string $pattern, string $replacement, string $subject): ?string
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_filter.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `preg_filter()`](../../../php/builtins/regex/preg_filter.md)
