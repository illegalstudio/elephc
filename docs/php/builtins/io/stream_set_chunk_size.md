---
title: "stream_set_chunk_size()"
description: "Lowers `stream_set_chunk_size(stream, size)` and returns the previous size."
sidebar:
  order: 195
---

## stream_set_chunk_size()

```php
function stream_set_chunk_size(resource $stream, int $size): int
```

Lowers `stream_set_chunk_size(stream, size)` and returns the previous size.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_chunk_size` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_chunk_size.md).

