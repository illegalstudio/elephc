---
title: "strcspn()"
description: "Returns the length of the initial byte span containing none of the selected characters."
sidebar:
  order: 1049
---

## strcspn()

```php
function strcspn(string $string, string $characters, int $offset = 0, ?int $length = null): int
```

Returns the length of the initial byte span containing none of the selected characters.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)
- `$offset` (`int`), default `0`, optional
- `$length` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strcspn.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strcspn.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strcspn` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcspn.md).
