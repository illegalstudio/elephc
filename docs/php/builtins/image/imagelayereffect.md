---
title: "imagelayereffect()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 515
---

## imagelayereffect()

```php
function imagelayereffect(mixed $image, int $effect): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$effect` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagelayereffect` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagelayereffect.md).
