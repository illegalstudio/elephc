---
title: "imageistruecolor()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 513
---

## imageistruecolor()

```php
function imageistruecolor(mixed $image): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageistruecolor` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageistruecolor.md).
