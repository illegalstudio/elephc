---
title: "disk_total_space()"
description: "Returns the total size of a filesystem or disk partition."
sidebar:
  order: 111
---

## disk_total_space()

```php
function disk_total_space(string $directory): float
```

Returns the total size of a filesystem or disk partition.

**Parameters**:
- `$directory` (`string`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/disk_total_space.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/disk_total_space.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `disk_total_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/disk_total_space.md).

