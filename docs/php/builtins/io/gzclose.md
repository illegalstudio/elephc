---
title: "gzclose()"
description: "Closes an open gz-file pointer."
sidebar:
  order: 200
---

## gzclose()

```php
function gzclose(mixed $stream): bool
```

Closes an open gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzclose` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzclose.md).
