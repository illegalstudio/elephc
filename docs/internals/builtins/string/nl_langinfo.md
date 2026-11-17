---
title: "nl_langinfo() - internals"
description: "Compiler internals for nl_langinfo(): lowering path, type checks, and runtime helpers."
---

## `nl_langinfo()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_locale.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_locale.rs)
- **Lowering**: [`src/locale_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/locale_prelude.rs) (`nl_langinfo`)
- **Function symbol**: `nl_langinfo()`


### Lowering notes

- Implemented by the compiler-injected locale prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function nl_langinfo(int $item): string|false
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/locale/nl_langinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/locale/nl_langinfo.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `nl_langinfo()`](../../../php/builtins/string/nl_langinfo.md)
