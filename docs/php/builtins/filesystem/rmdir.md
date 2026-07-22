---
title: "rmdir()"
description: "Removes a directory."
sidebar:
  order: 149
---

## rmdir()

```php
function rmdir(string $directory): bool
```

Removes a directory.

**Parameters**:
- `$directory` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/rmdir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rmdir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rmdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/rmdir.md).
