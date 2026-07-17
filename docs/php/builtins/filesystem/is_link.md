---
title: "is_link()"
description: "Tells whether the filename is a symbolic link."
sidebar:
  order: 129
---

## is_link()

```php
function is_link(string $filename): bool
```

Tells whether the filename is a symbolic link.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_link.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_link.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_link` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_link.md).

