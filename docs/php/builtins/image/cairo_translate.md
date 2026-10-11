---
title: "cairo_translate()"
description: "Translates the context's transformation by the given x and y offsets."
sidebar:
  order: 450
---

## cairo_translate()

```php
function cairo_translate(mixed $context, float $tx, float $ty): void
```

Translates the context's transformation by the given x and y offsets.

**Parameters**:
- `$context` (`mixed`)
- `$tx` (`float`)
- `$ty` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_translate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_translate.md).
