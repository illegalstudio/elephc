---
title: "gzgetc()"
description: "Gets one character from a gz-file pointer."
sidebar:
  order: 203
---

## gzgetc()

```php
function gzgetc(mixed $stream): mixed
```

Gets one character from a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzgetc` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzgetc.md).
