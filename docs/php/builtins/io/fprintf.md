---
title: "fprintf()"
description: "Write a formatted string to a stream."
sidebar:
  order: 171
---

## fprintf()

```php
function fprintf(resource $stream, string $format, ...$values): int
```

Write a formatted string to a stream.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fprintf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fprintf.md).

