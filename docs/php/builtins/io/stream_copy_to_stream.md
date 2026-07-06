---
title: "stream_copy_to_stream()"
description: "Copies data from one stream to another."
sidebar:
  order: 198
---

## stream_copy_to_stream()

```php
function stream_copy_to_stream(resource $from, resource $to, int $length = null, int $offset = -1): mixed
```

Copies data from one stream to another.

**Parameters**:
- `$from` (`resource`)
- `$to` (`resource`)
- `$length` (`int`), default `null`, optional
- `$offset` (`int`), default `-1`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_copy_to_stream` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_copy_to_stream.md).

