---
title: "gzcompress()"
description: "Lowers `gzcompress(data, level?)` through inline zlib `compress2` calls."
sidebar:
  order: 326
---

## gzcompress()

```php
function gzcompress(string $data, int $level, int $encoding): string
```

Lowers `gzcompress(data, level?)` through inline zlib `compress2` calls.

**Parameters**:
- `$data` (`string`)
- `$level` (`int`), optional
- `$encoding` (`int`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzcompress` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzcompress.md).

