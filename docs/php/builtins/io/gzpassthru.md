---
title: "gzpassthru()"
description: "Outputs all remaining data on a gz-file pointer."
sidebar:
  order: 206
---

## gzpassthru()

```php
function gzpassthru(mixed $stream): int
```

Outputs all remaining data on a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzpassthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzpassthru.md).
