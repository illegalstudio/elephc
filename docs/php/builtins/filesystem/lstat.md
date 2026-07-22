---
title: "lstat()"
description: "Gives information about a file or symbolic link."
sidebar:
  order: 139
---

## lstat()

```php
function lstat(string $filename): mixed
```

Gives information about a file or symbolic link.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/lstat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/lstat.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lstat` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lstat.md).
