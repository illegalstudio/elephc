---
title: "gzinflate()"
description: "Lowers `gzinflate(data, max_length?)` and boxes zlib failures as PHP false."
sidebar:
  order: 330
---

## gzinflate()

```php
function gzinflate(string $data, int $max_length): string
```

Lowers `gzinflate(data, max_length?)` and boxes zlib failures as PHP false.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzinflate` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzinflate.md).

