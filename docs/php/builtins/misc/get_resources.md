---
title: "get_resources()"
description: "Returns currently active resources, optionally filtered by type."
---

## get_resources()

```php
function get_resources(mixed $type = null): array
```

Returns currently active resources, optionally filtered by type.

**Parameters**:
- `$type` (`mixed`), default `null`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_resources` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_resources.md).
