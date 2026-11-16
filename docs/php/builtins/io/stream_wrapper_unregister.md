---
title: "stream_wrapper_unregister()"
description: "Unregisters a previously registered URL wrapper."
---

## stream_wrapper_unregister()

```php
function stream_wrapper_unregister(string $protocol): bool
```

Unregisters a previously registered URL wrapper.

**Parameters**:
- `$protocol` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_wrapper_unregister` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_unregister.md).
