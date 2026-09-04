---
title: "zlib_encode()"
description: "Compresses a string with the requested zlib framing."
sidebar:
  order: 553
---

## zlib_encode()

```php
function zlib_encode(string $data, int $encoding, int $level = -1): mixed
```

Compresses a string with the requested zlib framing.

**Parameters**:
- `$data` (`string`)
- `$encoding` (`int`)
- `$level` (`int`), default `-1`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zlib_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/zlib_encode.md).
