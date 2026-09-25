---
title: "cairo_pattern_add_color_stop_rgb()"
description: "Adds an opaque color stop to a gradient pattern."
sidebar:
  order: 427
---

## cairo_pattern_add_color_stop_rgb()

```php
function cairo_pattern_add_color_stop_rgb(mixed $pattern, float $offset, float $red, float $green, float $blue): void
```

Adds an opaque color stop to a gradient pattern.

**Parameters**:
- `$pattern` (`mixed`)
- `$offset` (`float`)
- `$red` (`float`)
- `$green` (`float`)
- `$blue` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_pattern_add_color_stop_rgb` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_pattern_add_color_stop_rgb.md).
