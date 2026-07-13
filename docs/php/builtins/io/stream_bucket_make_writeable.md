---
title: "stream_bucket_make_writeable()"
description: "Returns a bucket object from the brigade for use in a stream filter."
sidebar:
  order: 193
---

## stream_bucket_make_writeable()

```php
function stream_bucket_make_writeable(mixed $brigade): mixed
```

Returns a bucket object from the brigade for use in a stream filter.

**Parameters**:
- `$brigade` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_make_writeable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_make_writeable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_bucket_make_writeable` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_bucket_make_writeable.md).

