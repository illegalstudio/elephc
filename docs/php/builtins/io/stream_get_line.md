---
title: "stream_get_line()"
description: "Gets line from stream resource up to a given delimiter."
sidebar:
  order: 203
---

## stream_get_line()

```php
function stream_get_line(resource $stream, int $length, string $ending = ''): string
```

Gets line from stream resource up to a given delimiter.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`)
- `$ending` (`string`), default `''`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_get_line` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_line.md).

