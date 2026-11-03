---
title: "stream_get_meta_data()"
description: "Retrieves metadata from streams/file pointers."
---

## stream_get_meta_data()

```php
function stream_get_meta_data(resource $stream): array
```

Retrieves metadata from streams/file pointers.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_get_meta_data` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_meta_data.md).
