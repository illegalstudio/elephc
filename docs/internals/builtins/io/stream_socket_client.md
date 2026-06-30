---
title: "stream_socket_client() — internals"
description: "Compiler internals for stream_socket_client(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 215
---

## `stream_socket_client()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2397](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2397) (`lower_stream_socket_client`)
- **Function symbol**: `lower_stream_socket_client()`


### Lowering notes

- Lowers `stream_socket_client(address)` and records the connected host for TLS defaults.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stash_connect_host`
- `__rt_stream_socket_client`

## Signature summary

```php
function stream_socket_client(string $address, int $error_code, int $error_message, string $timeout, float $flags): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.
- **By-reference parameters**: `$error_code`, `$error_message`.

## Cross-references

- [User reference for `stream_socket_client()`](../../../php/builtins/io/stream_socket_client.md)

