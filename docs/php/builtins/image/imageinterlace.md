---
title: "imageinterlace()"
description: "Reads or sets whether an image is written interlaced."
sidebar:
  order: 519
---

## imageinterlace()

```php
function imageinterlace(mixed $image, ?bool $enable = null): int
```

Reads or sets whether an image is written interlaced.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`?bool`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageinterlace` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageinterlace.md).
