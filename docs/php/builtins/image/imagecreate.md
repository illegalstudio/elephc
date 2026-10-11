---
title: "imagecreate()"
description: "Creates an empty palette image of the given size."
sidebar:
  order: 490
---

## imagecreate()

```php
function imagecreate(int $width, int $height): mixed
```

Creates an empty palette image of the given size.

**Parameters**:
- `$width` (`int`)
- `$height` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreate.md).
