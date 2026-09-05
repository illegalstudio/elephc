---
title: "imageflip()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 506
---

## imageflip()

```php
function imageflip(mixed $image, int $mode): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$mode` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageflip` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageflip.md).
