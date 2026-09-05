---
title: "imageaffine()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 455
---

## imageaffine()

```php
function imageaffine(mixed $image, array $affine, ?array $clip = null): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$affine` (`array`)
- `$clip` (`?array`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageaffine` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageaffine.md).
