---
title: "gzgets()"
description: "Gets one line from a gz-file pointer."
sidebar:
  order: 204
---

## gzgets()

```php
function gzgets(mixed $stream, int $length = null): mixed
```

Gets one line from a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)
- `$length` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzgets` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzgets.md).
