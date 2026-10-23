---
title: "strcspn()"
description: "Finds the length of the initial segment not matching a mask."
sidebar:
  order: 927
---

## strcspn()

```php
function strcspn(string $string, string $characters, int $offset = 0, ?int $length = null): int
```

Finds the length of the initial segment not matching a mask.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)
- `$offset` (`int`), default `0`, optional
- `$length` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strcspn.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strcspn.rs)).

**Examples**:

// Full example: examples/string-ops/main.php
echo strcspn("key=value pair", " ");  // 9

## Internals

For how `strcspn` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcspn.md).
