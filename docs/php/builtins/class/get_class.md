---
title: "get_class()"
description: "Returns the name of the class of an object."
---

## get_class()

```php
function get_class(object $object = null): string
```

Returns the name of the class of an object.

**Parameters**:
- `$object` (`object`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class.md).
