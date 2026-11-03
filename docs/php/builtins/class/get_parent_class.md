---
title: "get_parent_class()"
description: "Returns the name of the parent class of an object or class."
---

## get_parent_class()

```php
function get_parent_class(mixed $object_or_class = null): string
```

Returns the name of the parent class of an object or class.

**Parameters**:
- `$object_or_class` (`mixed`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_parent_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_parent_class.md).
