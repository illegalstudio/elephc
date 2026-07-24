---
title: "tempnam()"
description: "Creates a file with a unique filename."
sidebar:
  order: 154
---

## tempnam()

```php
function tempnam(string $directory, string $prefix): mixed
```

Creates a file with a unique filename.

**Parameters**:
- `$directory` (`string`)
- `$prefix` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/tempnam.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/tempnam.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `tempnam` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/tempnam.md).
