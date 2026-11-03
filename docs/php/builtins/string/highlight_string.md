---
title: "highlight_string()"
description: "Syntax highlighting of a string."
sidebar:
  order: 924
---

## highlight_string()

```php
function highlight_string(string $string, bool $return = false): string|bool
```

Syntax highlighting of a string.

**Parameters**:
- `$string` (`string`)
- `$return` (`bool`), default `false`, optional

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/highlight_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/highlight_string.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `highlight_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/highlight_string.md).
