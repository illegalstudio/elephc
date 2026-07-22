---
title: "readlink()"
description: "Returns the target of a symbolic link."
sidebar:
  order: 144
---

## readlink()

```php
function readlink(string $path): mixed
```

Returns the target of a symbolic link.

**Parameters**:
- `$path` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/readlink.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/readlink.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/readlink.md).
