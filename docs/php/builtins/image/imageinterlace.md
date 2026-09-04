---
title: "imageinterlace()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 512
---

## imageinterlace()

```php
function imageinterlace(mixed $image, bool $enable = null): int
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`bool`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageinterlace` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageinterlace.md).
