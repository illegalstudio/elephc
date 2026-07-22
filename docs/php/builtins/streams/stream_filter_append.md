---
title: "stream_filter_append()"
description: "Attaches a filter to a stream."
sidebar:
  order: 356
---

## stream_filter_append()

```php
function stream_filter_append(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed
```

Attaches a filter to a stream.

**Parameters**:
- `$stream` (`resource`)
- `$filtername` (`string`)
- `$read_write` (`int`), default `3`, optional
- `$params` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_append.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_append.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_filter_append` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/stream_filter_append.md).
