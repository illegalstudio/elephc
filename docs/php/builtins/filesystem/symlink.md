---
title: "symlink()"
description: "Creates a symbolic link."
sidebar:
  order: 150
---

## symlink()

```php
function symlink(string $target, string $link): bool
```

Creates a symbolic link.

**Parameters**:
- `$target` (`string`)
- `$link` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/symlink.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/symlink.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `symlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/symlink.md).

