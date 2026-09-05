---
title: "cairo_pattern_create_radial()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 425
---

## cairo_pattern_create_radial()

```php
function cairo_pattern_create_radial(float $cx0, float $cy0, float $radius0, float $cx1, float $cy1, float $radius1): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$cx0` (`float`)
- `$cy0` (`float`)
- `$radius0` (`float`)
- `$cx1` (`float`)
- `$cy1` (`float`)
- `$radius1` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_pattern_create_radial` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_pattern_create_radial.md).
