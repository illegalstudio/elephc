---
title: "imagelayereffect()"
description: "Selects the alpha blending effect used by subsequent drawing."
sidebar:
  order: 522
---

## imagelayereffect()

```php
function imagelayereffect(mixed $image, int $effect): bool
```

Selects the alpha blending effect used by subsequent drawing.

**Parameters**:
- `$image` (`mixed`)
- `$effect` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagelayereffect` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagelayereffect.md).
