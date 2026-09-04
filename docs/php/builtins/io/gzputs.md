---
title: "gzputs()"
description: "Alias of gzwrite()."
sidebar:
  order: 207
---

## gzputs()

```php
function gzputs(mixed $stream, string $data, int $length = null): mixed
```

Alias of gzwrite().

**Parameters**:
- `$stream` (`mixed`)
- `$data` (`string`)
- `$length` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzputs` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzputs.md).
