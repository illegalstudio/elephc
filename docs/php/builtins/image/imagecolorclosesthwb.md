---
title: "imagecolorclosesthwb()"
description: "Returns the palette index closest in hue, whiteness, and blackness."
sidebar:
  order: 471
---

## imagecolorclosesthwb()

```php
function imagecolorclosesthwb(mixed $image, int $red, int $green, int $blue): int
```

Returns the palette index closest in hue, whiteness, and blackness.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorclosesthwb` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorclosesthwb.md).
