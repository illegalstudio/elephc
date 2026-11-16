---
title: "spl_autoload_unregister()"
description: "Unregister given function as __autoload() implementation."
---

## spl_autoload_unregister()

```php
function spl_autoload_unregister(callable $callback): bool
```

Unregister given function as __autoload() implementation.

**Parameters**:
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `spl_autoload_unregister` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_unregister.md).
