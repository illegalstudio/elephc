---
title: "get_debug_type()"
description: "Returns the resolved name of the given variable's type."
---

## get_debug_type()

```php
function get_debug_type(mixed $value): string
```

Returns the resolved name of the given variable's type.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_debug_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/get_debug_type.md).
