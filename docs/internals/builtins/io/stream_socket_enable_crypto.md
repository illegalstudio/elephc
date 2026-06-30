---
title: "stream_socket_enable_crypto() — internals"
description: "Compiler internals for stream_socket_enable_crypto(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 216
---

## `stream_socket_enable_crypto()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2547](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2547) (`lower_stream_socket_enable_crypto`)
- **Function symbol**: `lower_stream_socket_enable_crypto()`


### Lowering notes

- Lowers `stream_socket_enable_crypto(stream, enable, method?, session_stream?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method, resource $session_stream): bool
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `stream_socket_enable_crypto()`](../../../php/builtins/io/stream_socket_enable_crypto.md)

