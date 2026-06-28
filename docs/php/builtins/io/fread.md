---
title: "fread()"
description: "Lowers `fread(stream, length)` using the shared runtime file-read helper."
sidebar:
  order: 152
---

## fread()

```php
function fread(resource $stream, int $length): string
```

Lowers `fread(stream, length)` using the shared runtime file-read helper.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fread` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fread.md).

