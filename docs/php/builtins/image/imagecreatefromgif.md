---
title: "imagecreatefromgif()"
description: "Creates an image from a GIF file."
sidebar:
  order: 492
---

## imagecreatefromgif()

```php
function imagecreatefromgif(string $filename): mixed
```

Creates an image from a GIF file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromgif` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromgif.md).
