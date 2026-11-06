---
title: "property_exists()"
description: "Checks whether an object or class has a property."
---

## property_exists()

```php
function property_exists(mixed $object_or_class, string $property): bool
```

Checks whether an object or class has a property.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$property` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `property_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/property_exists.md).
