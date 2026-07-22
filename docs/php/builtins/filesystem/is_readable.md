---
title: "is_readable()"
description: "Tells whether the filename is readable."
sidebar:
  order: 132
---

## is_readable()

```php
function is_readable(string $filename): bool
```

Tells whether the filename is readable.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_readable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_readable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_readable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_readable.md).
