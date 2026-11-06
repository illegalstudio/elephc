---
title: "ob_implicit_flush()"
description: "Turns implicit flush on/off."
---

## ob_implicit_flush()

```php
function ob_implicit_flush(bool $enable = true): bool
```

Turns implicit flush on/off.

**Parameters**:
- `$enable` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_implicit_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_implicit_flush.md).
