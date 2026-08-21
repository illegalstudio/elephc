---
title: "diskfreespace()"
description: "Returns available space in filesystem or disk partition (alias of disk_free_space)."
sidebar:
  order: 168
---

## diskfreespace()

```php
function diskfreespace(string $directory): float
```

Returns available space in filesystem or disk partition (alias of disk_free_space).

**Parameters**:
- `$directory` (`string`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/diskfreespace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/diskfreespace.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `diskfreespace` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/diskfreespace.md).
