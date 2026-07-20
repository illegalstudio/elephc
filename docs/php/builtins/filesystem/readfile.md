---
title: "readfile()"
description: "Outputs a file."
sidebar:
  order: 141
---

## readfile()

```php
function readfile(string $filename): mixed
```

Outputs a file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/readfile.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/readfile.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readfile` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/readfile.md).

