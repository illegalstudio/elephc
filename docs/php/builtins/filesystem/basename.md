---
title: "basename()"
description: "Returns the trailing name component of a path."
sidebar:
  order: 104
---

## basename()

```php
function basename(string $path, string $suffix = ''): string
```

Returns the trailing name component of a path.

**Parameters**:
- `$path` (`string`)
- `$suffix` (`string`), default `''`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/basename.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/basename.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `basename` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/basename.md).
