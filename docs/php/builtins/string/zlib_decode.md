---
title: "zlib_decode()"
description: "Decompresses a raw, zlib or gzip framed string, detecting which."
sidebar:
  order: 552
---

## zlib_decode()

```php
function zlib_decode(string $data, int $max_length = 0): mixed
```

Decompresses a raw, zlib or gzip framed string, detecting which.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zlib_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/zlib_decode.md).
