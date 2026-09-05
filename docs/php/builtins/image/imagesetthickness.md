---
title: "imagesetthickness()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 529
---

## imagesetthickness()

```php
function imagesetthickness(mixed $image, int $thickness): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$thickness` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesetthickness` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesetthickness.md).
