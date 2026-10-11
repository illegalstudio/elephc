---
title: "Streams builtins"
description: "Builtins in the Streams category."
sidebar:
  order: 111
---

## Streams builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`fsockopen()`](./streams/fsockopen.md) | `(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): mixed` | `mixed` | ✓ | ✓ |
| [`pfsockopen()`](./streams/pfsockopen.md) | `(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): mixed` | `mixed` | ✓ | ✓ |
| [`stream_bucket_append()`](./streams/stream_bucket_append.md) | `(mixed $brigade, mixed $bucket): void` | `void` | ✓ | ✓ |
| [`stream_bucket_prepend()`](./streams/stream_bucket_prepend.md) | `(mixed $brigade, mixed $bucket): void` | `void` | ✓ | ✓ |
| [`stream_filter_append()`](./streams/stream_filter_append.md) | `(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed` | `mixed` | ✓ | ✓ |
| [`stream_filter_prepend()`](./streams/stream_filter_prepend.md) | `(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed` | `mixed` | ✓ | ✓ |
