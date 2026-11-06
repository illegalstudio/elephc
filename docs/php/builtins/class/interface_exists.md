---
title: "interface_exists()"
description: "Checks if the interface has been defined."
---

## interface_exists()

```php
function interface_exists(string $interface, bool $autoload = true): bool
```

Checks if the interface has been defined.

**Parameters**:
- `$interface` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `interface_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/interface_exists.md).
