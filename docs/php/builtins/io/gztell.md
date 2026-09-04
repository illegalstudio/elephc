---
title: "gztell()"
description: "Tells the read/write position of a gz-file pointer."
sidebar:
  order: 211
---

## gztell()

```php
function gztell(mixed $stream): mixed
```

Tells the read/write position of a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gztell` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gztell.md).
