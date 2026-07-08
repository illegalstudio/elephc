---
title: "stream_filter_prepend()"
description: "Attaches a filter to a stream (prepend)."
sidebar:
  order: 337
---

## stream_filter_prepend()

```php
function stream_filter_prepend(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed
```

Attaches a filter to a stream (prepend).

**Parameters**:
- `$stream` (`resource`)
- `$filtername` (`string`)
- `$read_write` (`int`), default `3`, optional
- `$params` (`mixed`), default `null`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_filter_prepend` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/stream_filter_prepend.md).

