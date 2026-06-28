---
title: "Streams builtins"
description: "Builtins in the Streams category."
sidebar:
  order: 111
---

## Streams builtins

| Function | Signature | Returns |
|---|---|---|
| [`fsockopen()`](./streams/fsockopen.md) | `(string $hostname, int $port, int $error_code, string $error_message, float $timeout): mixed` | `mixed` |
| [`pfsockopen()`](./streams/pfsockopen.md) | `(string $hostname, int $port, int $error_code, string $error_message, float $timeout): mixed` | `mixed` |
| [`stream_bucket_append()`](./streams/stream_bucket_append.md) | `(mixed $brigade, mixed $bucket): void` | `void` |
| [`stream_bucket_prepend()`](./streams/stream_bucket_prepend.md) | `(mixed $brigade, mixed $bucket): void` | `void` |
| [`stream_filter_append()`](./streams/stream_filter_append.md) | `(resource $stream, string $filter_name, int $mode, mixed $params): mixed` | `mixed` |
| [`stream_filter_prepend()`](./streams/stream_filter_prepend.md) | `(resource $stream, string $filter_name, int $mode, mixed $params): mixed` | `mixed` |
