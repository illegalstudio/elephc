---
title: "ctype_space()"
description: "Checks if all characters in the string are whitespace characters."
sidebar:
  order: 432
---

## ctype_space()

```php
function ctype_space(string $text): bool
```

Checks if all characters in the string are whitespace characters.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_space.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_space.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ctype_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/ctype_space.md).

