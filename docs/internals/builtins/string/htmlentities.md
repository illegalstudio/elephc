---
title: "htmlentities() — internals"
description: "Compiler internals for htmlentities(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 366
---

## `htmlentities()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/htmlentities.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/htmlentities.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:93](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L93) (`lower_html_escape`)
- **Function symbol**: `lower_html_escape()`


### Lowering notes

- Lowers `htmlspecialchars()` / `htmlentities()` — escapes the subject string (operand 0).
- `name` is the calling builtin's PHP name, used in argument-coercion diagnostics. The
- optional `flags` and `encoding` arguments are accepted (so the common `htmlspecialchars($s,
- ENT_QUOTES)` call form compiles) but not applied: `__rt_htmlspecialchars` implements the
- ENT_QUOTES behaviour, which matches PHP's default flag set and the overwhelmingly-common
- ENT_QUOTES call. (A flag-aware runtime — doctype-dependent `&apos;` vs `&#039;` — is a follow-up.)

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_grapheme_strrev`
- `__rt_htmlspecialchars`
- `__rt_strcopy`

## Signature summary

```php
function htmlentities(string $string, int $flags = 11, string $encoding = 'UTF-8'): string
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/htmlentities.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/htmlentities.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `htmlentities()`](../../../php/builtins/string/htmlentities.md)
