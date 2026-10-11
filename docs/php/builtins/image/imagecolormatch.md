---
title: "imagecolormatch()"
description: "Adjusts a palette image's colors to better match a truecolor original."
sidebar:
  order: 477
---

## imagecolormatch()

```php
function imagecolormatch(mixed $image1, mixed $image2): bool
```

Adjusts a palette image's colors to better match a truecolor original.

**Parameters**:
- `$image1` (`mixed`)
- `$image2` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolormatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolormatch.md).
