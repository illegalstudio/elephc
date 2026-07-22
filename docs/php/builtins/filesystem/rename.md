---
title: "rename()"
description: "Renames a file or directory."
sidebar:
  order: 148
---

## rename()

```php
function rename(string $from, string $to): bool
```

Renames a file or directory.

**Parameters**:
- `$from` (`string`)
- `$to` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/rename.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rename.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rename` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/rename.md).
