---
title: "imagebmp()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 460
---

## imagebmp()

```php
function imagebmp(mixed $image, ?string $file = null, bool $compressed = true): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`?string`), default `null`, optional
- `$compressed` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagebmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagebmp.md).
