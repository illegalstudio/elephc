---
title: "cairo_pattern_create_rgb()"
description: "Creates a solid opaque color pattern."
sidebar:
  order: 429
---

## cairo_pattern_create_rgb()

```php
function cairo_pattern_create_rgb(float $red, float $green, float $blue): mixed
```

Creates a solid opaque color pattern.

**Parameters**:
- `$red` (`float`)
- `$green` (`float`)
- `$blue` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_pattern_create_rgb` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_pattern_create_rgb.md).
