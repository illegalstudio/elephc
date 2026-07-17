---
title: "file_put_contents()"
description: "Writes data to a file."
sidebar:
  order: 167
---

## file_put_contents()

```php
function file_put_contents(string $filename, string $data): int
```

Writes data to a file.

**Parameters**:
- `$filename` (`string`)
- `$data` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/file_put_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_put_contents.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file_put_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file_put_contents.md).

