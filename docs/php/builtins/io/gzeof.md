---
title: "gzeof()"
description: "Tests for end-of-file on a gz-file pointer."
sidebar:
  order: 201
---

## gzeof()

```php
function gzeof(mixed $stream): bool
```

Tests for end-of-file on a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzeof.md).
