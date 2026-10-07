---
title: "imageistruecolor()"
description: "Reports whether an image is truecolor rather than palette-based."
sidebar:
  order: 520
---

## imageistruecolor()

```php
function imageistruecolor(mixed $image): bool
```

Reports whether an image is truecolor rather than palette-based.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageistruecolor` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageistruecolor.md).
