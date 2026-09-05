---
title: "imagecreate()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 485
---

## imagecreate()

```php
function imagecreate(int $width, int $height): mixed
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

For how `imagecreate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreate.md).
