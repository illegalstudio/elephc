---
title: "imagecolorsforindex()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 476
---

## imagecolorsforindex()

```php
function imagecolorsforindex(mixed $image, int $color): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorsforindex` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorsforindex.md).
