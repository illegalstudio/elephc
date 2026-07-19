---
title: "grapheme_strrev() — internals"
description: "Compiler internals for grapheme_strrev(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 364
---

## `grapheme_strrev()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/grapheme_strrev.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/grapheme_strrev.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:105](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L105) (`lower_grapheme_strrev`)
- **Function symbol**: `lower_grapheme_strrev()`


### Lowering notes

- Lowers `grapheme_strrev()` and boxes its `string|false` result as `Mixed`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_grapheme_strrev`
- `__rt_strcopy`

## Signature summary

```php
function grapheme_strrev(string $string): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/grapheme_strrev.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/grapheme_strrev.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `grapheme_strrev()`](../../../php/builtins/string/grapheme_strrev.md)
