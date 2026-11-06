---
title: "stream_set_chunk_size()"
description: "Sets the read chunk size on a stream."
---

## stream_set_chunk_size()

```php
function stream_set_chunk_size(resource $stream, int $size): int
```

Sets the read chunk size on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_set_chunk_size` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_chunk_size.md).
