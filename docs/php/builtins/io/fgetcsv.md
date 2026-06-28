---
title: "fgetcsv()"
description: "Lowers `fgetcsv(stream, separator?, enclosure?)` through the CSV row runtime helper."
sidebar:
  order: 142
---

## fgetcsv()

```php
function fgetcsv(resource $stream, int $length, string $separator, string $enclosure, string $escape): array
```

Lowers `fgetcsv(stream, separator?, enclosure?)` through the CSV row runtime helper.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`), optional
- `$separator` (`string`), optional
- `$enclosure` (`string`)
- `$escape` (`string`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fgetcsv` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fgetcsv.md).

