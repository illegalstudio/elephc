---
title: "imagecolormatch()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 472
---

## imagecolormatch()

```php
function imagecolormatch(mixed $image1, mixed $image2): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image1` (`mixed`)
- `$image2` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolormatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolormatch.md).
