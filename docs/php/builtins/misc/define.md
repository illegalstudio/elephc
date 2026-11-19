---
title: "define()"
description: "Defines a named constant at runtime."
---

## define()

```php
function define(string $constant_name, mixed $value, bool $case_insensitive = false): bool
```

Defines a named constant at runtime.

**Parameters**:
- `$constant_name` (`string`)
- `$value` (`mixed`)
- `$case_insensitive` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `define` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/define.md).
