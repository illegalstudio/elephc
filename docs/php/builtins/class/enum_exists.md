---
title: "enum_exists()"
description: "Checks if the enum has been defined."
---

## enum_exists()

```php
function enum_exists(string $enum, bool $autoload = true): bool
```

Checks if the enum has been defined.

**Parameters**:
- `$enum` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `enum_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/enum_exists.md).
