---
title: "stream_set_timeout()"
description: "Lowers `stream_set_timeout(stream, seconds, microseconds?)`."
sidebar:
  order: 197
---

## stream_set_timeout()

```php
function stream_set_timeout(resource $stream, int $seconds, int $microseconds): bool
```

Lowers `stream_set_timeout(stream, seconds, microseconds?)`.

**Parameters**:
- `$stream` (`resource`)
- `$seconds` (`int`)
- `$microseconds` (`int`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_timeout` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_timeout.md).

