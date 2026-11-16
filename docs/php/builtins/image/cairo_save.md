---
title: "cairo_save()"
description: "Saves the context state so a later cairo_restore() can return to it."
---

## cairo_save()

```php
function cairo_save(mixed $context): void
```

Saves the context state so a later cairo_restore() can return to it.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_save` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_save.md).
