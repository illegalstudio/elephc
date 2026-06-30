---
title: "gzdeflate()"
description: "Lowers `gzdeflate(data, level?)` through inline raw-DEFLATE zlib calls."
sidebar:
  order: 344
---

## gzdeflate()

```php
function gzdeflate(string $data, int $level, int $encoding): string
```

Lowers `gzdeflate(data, level?)` through inline raw-DEFLATE zlib calls.

**Parameters**:
- `$data` (`string`)
- `$level` (`int`), optional
- `$encoding` (`int`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gzdeflate` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzdeflate.md).

