---
title: "stream_get_meta_data()"
description: "Retrieves metadata from streams/file pointers."
sidebar:
  order: 223
---

## stream_get_meta_data()

```php
function stream_get_meta_data(resource $stream): array
```

Retrieves metadata from streams/file pointers.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_meta_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_get_meta_data.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_get_meta_data` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_meta_data.md).
