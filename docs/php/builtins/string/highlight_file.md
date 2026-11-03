---
title: "highlight_file()"
description: "Syntax highlighting of a file."
sidebar:
  order: 921
---

## highlight_file()

```php
function highlight_file(string $filename, bool $return = false): string|bool
```

Syntax highlighting of a file.

**Parameters**:
- `$filename` (`string`)
- `$return` (`bool`), default `false`, optional

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/highlight_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/highlight_file.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `highlight_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/highlight_file.md).
