---
title: "opendir()"
description: "Open directory handle."
sidebar:
  order: 189
---

## opendir()

```php
function opendir(string $directory): mixed
```

Open directory handle.

**Parameters**:
- `$directory` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/opendir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/opendir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `opendir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/opendir.md).

