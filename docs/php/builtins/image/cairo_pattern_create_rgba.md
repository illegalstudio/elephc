---
title: "cairo_pattern_create_rgba()"
description: "Creates a solid color pattern with alpha."
sidebar:
  order: 432
---

## cairo_pattern_create_rgba()

```php
function cairo_pattern_create_rgba(float $red, float $green, float $blue, float $alpha): mixed
```

Creates a solid color pattern with alpha.

**Parameters**:
- `$red` (`float`)
- `$green` (`float`)
- `$blue` (`float`)
- `$alpha` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_pattern_create_rgba` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_pattern_create_rgba.md).
