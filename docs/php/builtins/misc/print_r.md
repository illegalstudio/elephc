---
title: "print_r()"
description: "Prints human-readable information about a variable."
---

## print_r()

```php
function print_r(mixed $value, bool $return = false): mixed
```

Prints human-readable information about a variable.

**Parameters**:
- `$value` (`mixed`)
- `$return` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `print_r` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/print_r.md).
