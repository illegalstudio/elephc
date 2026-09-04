---
title: "imagetruecolortopalette()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 534
---

## imagetruecolortopalette()

```php
function imagetruecolortopalette(mixed $image, bool $dither, int $num_colors): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$dither` (`bool`)
- `$num_colors` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagetruecolortopalette` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagetruecolortopalette.md).
