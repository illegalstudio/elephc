---
title: "filectime()"
description: "Gets inode change time of file."
sidebar:
  order: 114
---

## filectime()

```php
function filectime(string $filename): mixed
```

Gets inode change time of file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/filectime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filectime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `filectime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filectime.md).

