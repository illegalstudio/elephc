---
title: "gzdecode()"
description: "Decodes a gzip-framed string."
sidebar:
  order: 461
---

## gzdecode()

```php
function gzdecode(string $data, int $max_length = 0): mixed
```

Decodes a gzip-framed string.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzdecode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzdecode.md).
