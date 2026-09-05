---
title: "imagetypes()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 535
---

## imagetypes()

```php
function imagetypes(): int
```

Implemented by the compiler-injected image prelude.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagetypes` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagetypes.md).
