---
title: "strspn()"
description: "Finds the length of the initial segment matching a mask."
sidebar:
  order: 939
---

## strspn()

```php
function strspn(string $string, string $characters, int $offset = 0, ?int $length = null): int
```

Finds the length of the initial segment matching a mask.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)
- `$offset` (`int`), default `0`, optional
- `$length` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strspn.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strspn.rs)).

**Examples**:

// Full example: examples/string-ops/main.php
echo strspn("  42 apples", " ");                // 2
echo strspn("  42 apples", "0123456789", 2);  // 2

## Internals

For how `strspn` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strspn.md).
