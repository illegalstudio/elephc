---
title: "stream_get_contents()"
description: "Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`."
sidebar:
  order: 184
---

## stream_get_contents()

```php
function stream_get_contents(resource $stream, int $length, int $offset): mixed
```

Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`), optional
- `$offset` (`int`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_get_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_contents.md).

