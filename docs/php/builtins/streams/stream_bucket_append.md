---
title: "stream_bucket_append()"
description: "Appends a bucket to the brigade."
sidebar:
  order: 354
---

## stream_bucket_append()

```php
function stream_bucket_append(mixed $brigade, mixed $bucket): void
```

Appends a bucket to the brigade.

**Parameters**:
- `$brigade` (`mixed`)
- `$bucket` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_append.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_bucket_append.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_bucket_append` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/stream_bucket_append.md).
