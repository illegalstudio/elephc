---
title: "gzuncompress()"
description: "Lowers `gzuncompress(data, max_length?)` and boxes zlib failures as PHP false."
sidebar:
  order: 346
---

## gzuncompress()

```php
function gzuncompress(string $data, int $max_length): string
```

Lowers `gzuncompress(data, max_length?)` and boxes zlib failures as PHP false.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzuncompress` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzuncompress.md).

