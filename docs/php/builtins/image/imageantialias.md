---
title: "imageantialias()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 458
---

## imageantialias()

```php
function imageantialias(mixed $image, bool $enable): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageantialias` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageantialias.md).
