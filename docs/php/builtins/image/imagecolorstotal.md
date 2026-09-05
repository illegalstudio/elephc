---
title: "imagecolorstotal()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 477
---

## imagecolorstotal()

```php
function imagecolorstotal(mixed $image): int
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorstotal` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorstotal.md).
