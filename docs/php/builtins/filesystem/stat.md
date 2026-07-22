---
title: "stat()"
description: "Gives information about a file."
sidebar:
  order: 151
---

## stat()

```php
function stat(string $filename): mixed
```

Gives information about a file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stat.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stat` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/stat.md).
