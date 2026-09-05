---
title: "imagecolortransparent()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 478
---

## imagecolortransparent()

```php
function imagecolortransparent(mixed $image, ?int $color = null): int
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolortransparent` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolortransparent.md).
