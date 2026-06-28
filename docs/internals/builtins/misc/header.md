---
title: "header() — internals"
description: "Compiler internals for header(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 263
---

## `header()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:289](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L289) (`lower_header`)
- **Function symbol**: `lower_header()`


### Lowering notes

- Lowers `header($line[, $replace[, $code]])` to `__rt_header`, materializing the
- four C-ABI integer arguments: arg0=line ptr, arg1=line len, arg2=`$replace`
- (default true), arg3=`$response_code` (default 0). `$replace`/`$code` are staged
- to scratch first (their evaluation may call helpers that clobber the string
- registers), then the line string is loaded and the staged ints reloaded into
- arg2/arg3. All PHP `header()` behavior lives in the bridge (`elephc_web_header`).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_header`

## Signature summary

```php
function header(mixed $header, mixed $replace, mixed $response_code): void
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `header()`](../../../php/builtins/misc/header.md)

