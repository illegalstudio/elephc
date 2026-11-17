---
title: "get_called_class()"
description: "Returns the late-static-binding class name."
---

## get_called_class()

```php
function get_called_class(): string
```

Returns the late-static-binding class name.

**Parameters**: none.

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_called_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_called_class.md).
