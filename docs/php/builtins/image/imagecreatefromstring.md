---
title: "imagecreatefromstring()"
description: "Creates an image from encoded bytes, detecting the format."
sidebar:
  order: 493
---

## imagecreatefromstring()

```php
function imagecreatefromstring(string $data): mixed
```

Creates an image from encoded bytes, detecting the format.

**Parameters**:
- `$data` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromstring` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromstring.md).
