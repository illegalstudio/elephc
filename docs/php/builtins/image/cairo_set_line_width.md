---
title: "cairo_set_line_width()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 436
---

## cairo_set_line_width()

```php
function cairo_set_line_width(mixed $context, float $width): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$width` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_line_width` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_line_width.md).
