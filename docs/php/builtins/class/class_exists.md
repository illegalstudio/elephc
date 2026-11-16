---
title: "class_exists()"
description: "Checks whether the given class has been defined."
---

## class_exists()

```php
function class_exists(string $class, bool $autoload = true): bool
```

Checks whether the given class has been defined.

**Parameters**:
- `$class` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `class_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_exists.md).
