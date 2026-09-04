---
title: "imagecreatetruecolor()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 493
---

## imagecreatetruecolor()

```php
function imagecreatetruecolor(int $width, int $height): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$width` (`int`)
- `$height` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatetruecolor` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatetruecolor.md).
