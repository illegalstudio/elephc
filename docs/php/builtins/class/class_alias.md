---
title: "class_alias()"
description: "Creates an alias for a class."
---

## class_alias()

```php
function class_alias(string $class, string $alias, bool $autoload = true): bool
```

Creates an alias for a class.

**Parameters**:
- `$class` (`string`)
- `$alias` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `class_alias` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_alias.md).
