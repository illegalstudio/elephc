---
title: "filegroup()"
description: "Gets file group."
sidebar:
  order: 124
---

## filegroup()

```php
function filegroup(string $filename): mixed
```

Gets file group.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/filegroup.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filegroup.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filegroup` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filegroup.md).
