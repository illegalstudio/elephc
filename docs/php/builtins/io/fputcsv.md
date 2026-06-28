---
title: "fputcsv()"
description: "Lowers `fputcsv(stream, fields, separator?, enclosure?)` for string arrays."
sidebar:
  order: 151
---

## fputcsv()

```php
function fputcsv(resource $stream, array $fields, string $separator = ',', string $enclosure = '"', string $escape = '\\', string $eol = '\n'): int
```

Lowers `fputcsv(stream, fields, separator?, enclosure?)` for string arrays.

**Parameters**:
- `$stream` (`resource`)
- `$fields` (`array`)
- `$separator` (`string`), default `','`, optional
- `$enclosure` (`string`), default `'"'`, optional
- `$escape` (`string`), default `'\\'`, optional
- `$eol` (`string`), default `'\n'`, optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fputcsv` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fputcsv.md).

