---
title: "imagecreatefromstring()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 490
---

## imagecreatefromstring()

```php
function imagecreatefromstring(string $data): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$data` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromstring` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromstring.md).
