---
title: "stream_bucket_prepend()"
description: "Prepends a bucket to the brigade."
sidebar:
  order: 360
---

## stream_bucket_prepend()

```php
function stream_bucket_prepend(mixed $brigade, mixed $bucket): void
```

Prepends a bucket to the brigade.

**Parameters**:
- `$brigade` (`mixed`)
- `$bucket` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_prepend.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_prepend.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_bucket_prepend` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/stream_bucket_prepend.md).
