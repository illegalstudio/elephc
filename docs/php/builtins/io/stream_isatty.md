---
title: "stream_isatty()"
description: "Checks if a stream is a TTY."
sidebar:
  order: 225
---

## stream_isatty()

```php
function stream_isatty(resource $stream): bool
```

Checks if a stream is a TTY.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_isatty.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_isatty.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_isatty` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_isatty.md).

