---
title: "wordwrap() — internals"
description: "Compiler internals for wordwrap(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 407
---

## `wordwrap()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:802](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L802) (`lower_wordwrap`)
- **Function symbol**: `lower_wordwrap()`


### Lowering notes

- Lowers `wordwrap(string, width?, break?, cut?)` through the shared runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_str_pad`
- `__rt_wordwrap`

## Signature summary

```php
function wordwrap(string $string, int $width, string $break, bool $cut_long_words): string
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Cross-references

- [User reference for `wordwrap()`](../../../php/builtins/string/wordwrap.md)

