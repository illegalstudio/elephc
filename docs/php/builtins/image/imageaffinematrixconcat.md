---
title: "imageaffinematrixconcat()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 456
---

## imageaffinematrixconcat()

```php
function imageaffinematrixconcat(mixed $matrix1, mixed $matrix2): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$matrix1` (`mixed`)
- `$matrix2` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageaffinematrixconcat` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageaffinematrixconcat.md).
