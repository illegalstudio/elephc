---
title: "fileperms()"
description: "Gets file permissions."
sidebar:
  order: 121
---

## fileperms()

```php
function fileperms(string $filename): mixed
```

Gets file permissions.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fileperms.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileperms.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileperms` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileperms.md).
