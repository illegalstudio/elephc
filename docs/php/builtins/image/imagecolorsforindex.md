---
title: "imagecolorsforindex()"
description: "Returns the red, green, blue, and alpha channels of a palette index."
sidebar:
  order: 481
---

## imagecolorsforindex()

```php
function imagecolorsforindex(mixed $image, int $color): array
```

Returns the red, green, blue, and alpha channels of a palette index.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`int`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorsforindex` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorsforindex.md).
