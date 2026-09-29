---
title: "imagecolortransparent()"
description: "Reads or sets an image's transparent color index."
sidebar:
  order: 481
---

## imagecolortransparent()

```php
function imagecolortransparent(mixed $image, ?int $color = null): int
```

Reads or sets an image's transparent color index.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolortransparent` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolortransparent.md).
