---
title: "cairo_restore()"
description: "Restores the context state saved by the matching cairo_save()."
---

## cairo_restore()

```php
function cairo_restore(mixed $context): void
```

Restores the context state saved by the matching cairo_save().

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_restore` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_restore.md).
