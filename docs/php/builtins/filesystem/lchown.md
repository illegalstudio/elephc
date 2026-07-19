---
title: "lchown()"
description: "Changes user ownership of a symlink."
sidebar:
  order: 134
---

## lchown()

```php
function lchown(string $filename, string $user): bool
```

Changes user ownership of a symlink.

**Parameters**:
- `$filename` (`string`)
- `$user` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/lchown.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/lchown.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lchown` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lchown.md).

