---
title: "disk_free_space()"
description: "Returns available space on filesystem or disk partition."
sidebar:
  order: 112
---

## disk_free_space()

```php
function disk_free_space(string $directory): float
```

Returns available space on filesystem or disk partition.

**Parameters**:
- `$directory` (`string`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/disk_free_space.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/disk_free_space.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `disk_free_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/disk_free_space.md).
