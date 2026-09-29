---
title: "cairo_pattern_add_color_stop_rgba()"
description: "Adds a color stop with alpha to a gradient pattern."
sidebar:
  order: 426
---

## cairo_pattern_add_color_stop_rgba()

```php
function cairo_pattern_add_color_stop_rgba(mixed $pattern, float $offset, float $red, float $green, float $blue, float $alpha): void
```

Adds a color stop with alpha to a gradient pattern.

**Parameters**:
- `$pattern` (`mixed`)
- `$offset` (`float`)
- `$red` (`float`)
- `$green` (`float`)
- `$blue` (`float`)
- `$alpha` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_pattern_add_color_stop_rgba` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_pattern_add_color_stop_rgba.md).
