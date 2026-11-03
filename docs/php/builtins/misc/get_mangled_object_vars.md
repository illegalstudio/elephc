---
title: "get_mangled_object_vars()"
description: "Returns an object's properties using PHP's visibility-mangled keys."
---

## get_mangled_object_vars()

```php
function get_mangled_object_vars(mixed $object): array
```

Returns an object's properties using PHP's visibility-mangled keys.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_mangled_object_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_mangled_object_vars.md).
