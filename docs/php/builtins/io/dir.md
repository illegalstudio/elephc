---
title: "dir()"
description: "Opens a directory and returns a Directory object, or false."
sidebar:
  order: 167
---

## dir()

```php
function dir(string $directory, mixed $context = null): mixed
```

Opens a directory and returns a Directory object, or false.

**Parameters**:
- `$directory` (`string`)
- `$context` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through a compiler-injected PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `dir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/dir.md).
