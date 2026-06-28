---
title: "stream_get_line()"
description: "Lowers `stream_get_line(stream, length, ending?)`."
sidebar:
  order: 186
---

## stream_get_line()

```php
function stream_get_line(resource $stream, int $length, string $ending): string
```

Lowers `stream_get_line(stream, length, ending?)`.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`)
- `$ending` (`string`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_get_line` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_line.md).

