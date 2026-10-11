---
title: "imagecreatefromtga()"
description: "Creates an image from a TGA file."
sidebar:
  order: 496
---

## imagecreatefromtga()

```php
function imagecreatefromtga(string $filename): mixed
```

Creates an image from a TGA file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromtga` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromtga.md).
