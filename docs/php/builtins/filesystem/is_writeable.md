---
title: "is_writeable()"
description: "Tells whether the filename is writable (alias of is_writable)."
sidebar:
  order: 134
---

## is_writeable()

```php
function is_writeable(string $filename): bool
```

Tells whether the filename is writable (alias of is_writable).

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_writeable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_writeable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_writeable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_writeable.md).
