---
title: "class_uses()"
description: "Returns the traits used by the given class."
---

## class_uses()

```php
function class_uses(mixed $object_or_class, bool $autoload = true): mixed
```

Returns the traits used by the given class.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `class_uses` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_uses.md).
