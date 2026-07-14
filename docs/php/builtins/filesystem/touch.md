---
title: "touch()"
description: "Sets access and modification time of a file."
sidebar:
  order: 154
---

## touch()

```php
function touch(string $filename, int $mtime = null, int $atime = null): bool
```

Sets access and modification time of a file.

**Parameters**:
- `$filename` (`string`)
- `$mtime` (`int`), default `null`, optional
- `$atime` (`int`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/touch.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/touch.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `touch` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/touch.md).

