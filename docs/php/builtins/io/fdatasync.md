---
title: "fdatasync()"
description: "Synchronizes data (but not meta-data) to file."
sidebar:
  order: 161
---

## fdatasync()

```php
function fdatasync(resource $stream): bool
```

Synchronizes data (but not meta-data) to file.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fdatasync.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fdatasync.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fdatasync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fdatasync.md).
