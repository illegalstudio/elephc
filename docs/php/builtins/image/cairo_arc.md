---
title: "cairo_arc()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 398
---

## cairo_arc()

```php
function cairo_arc(mixed $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$xc` (`float`)
- `$yc` (`float`)
- `$radius` (`float`)
- `$angle1` (`float`)
- `$angle2` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_arc` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_arc.md).
