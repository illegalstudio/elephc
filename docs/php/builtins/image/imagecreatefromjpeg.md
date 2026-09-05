---
title: "imagecreatefromjpeg()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 488
---

## imagecreatefromjpeg()

```php
function imagecreatefromjpeg(string $filename): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromjpeg` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromjpeg.md).
