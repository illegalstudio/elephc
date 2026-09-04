---
title: "imageresolution()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 523
---

## imageresolution()

```php
function imageresolution(mixed $image, int $resolution_x = null, int $resolution_y = null): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$resolution_x` (`int`), default `null`, optional
- `$resolution_y` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageresolution` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageresolution.md).
