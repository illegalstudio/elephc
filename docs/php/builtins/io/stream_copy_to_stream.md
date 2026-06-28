---
title: "stream_copy_to_stream()"
description: "Lowers `stream_copy_to_stream(from, to, length?, offset?)` through wrapper-aware read/write loops."
sidebar:
  order: 181
---

## stream_copy_to_stream()

```php
function stream_copy_to_stream(resource $from, resource $to, int $length, int $offset): mixed
```

Lowers `stream_copy_to_stream(from, to, length?, offset?)` through wrapper-aware read/write loops.

**Parameters**:
- `$from` (`resource`)
- `$to` (`resource`)
- `$length` (`int`), optional
- `$offset` (`int`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_copy_to_stream` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_copy_to_stream.md).

