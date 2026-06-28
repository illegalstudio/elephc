---
title: "stream_set_read_buffer()"
description: "Lowers stream read/write buffer setters as successful no-ops."
sidebar:
  order: 196
---

## stream_set_read_buffer()

```php
function stream_set_read_buffer(resource $stream, int $size): int
```

Lowers stream read/write buffer setters as successful no-ops.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_read_buffer` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_read_buffer.md).

