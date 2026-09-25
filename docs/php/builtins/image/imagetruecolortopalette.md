---
title: "imagetruecolortopalette()"
description: "Converts a truecolor image to a palette image in place."
sidebar:
  order: 541
---

## imagetruecolortopalette()

```php
function imagetruecolortopalette(mixed $image, bool $dither, int $num_colors): bool
```

Converts a truecolor image to a palette image in place.

**Parameters**:
- `$image` (`mixed`)
- `$dither` (`bool`)
- `$num_colors` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagetruecolortopalette` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagetruecolortopalette.md).
