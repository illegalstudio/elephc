---
title: "stream_bucket_new()"
description: "Lowers `stream_bucket_new(stream, data)` into a stdClass-backed bucket object."
sidebar:
  order: 173
---

## stream_bucket_new()

```php
function stream_bucket_new(resource $stream, string $buffer): mixed
```

Lowers `stream_bucket_new(stream, data)` into a stdClass-backed bucket object.

**Parameters**:
- `$stream` (`resource`)
- `$buffer` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_bucket_new` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_bucket_new.md).

