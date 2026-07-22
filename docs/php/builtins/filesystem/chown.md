---
title: "chown()"
description: "Changes file owner."
sidebar:
  order: 108
---

## chown()

```php
function chown(string $filename, string $user): bool
```

Changes file owner.

**Parameters**:
- `$filename` (`string`)
- `$user` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/chown.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/chown.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chown` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chown.md).
