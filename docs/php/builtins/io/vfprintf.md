---
title: "vfprintf()"
description: "Write a formatted string to a stream."
sidebar:
  order: 248
---

## vfprintf()

```php
function vfprintf(resource $stream, string $format, array $values): int
```

Write a formatted string to a stream.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/vfprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/vfprintf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `vfprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/vfprintf.md).
