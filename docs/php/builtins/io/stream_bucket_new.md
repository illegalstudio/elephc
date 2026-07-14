---
title: "stream_bucket_new()"
description: "Creates a new bucket for use in a stream filter."
sidebar:
  order: 194
---

## stream_bucket_new()

```php
function stream_bucket_new(resource $stream, string $buffer): mixed
```

Creates a new bucket for use in a stream filter.

**Parameters**:
- `$stream` (`resource`)
- `$buffer` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_new.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_new.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_bucket_new` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_bucket_new.md).

