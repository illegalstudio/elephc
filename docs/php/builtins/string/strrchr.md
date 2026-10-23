---
title: "strrchr()"
description: "Finds the last occurrence of a character in a string."
sidebar:
  order: 935
---

## strrchr()

```php
function strrchr(string $haystack, string $needle, bool $before_needle = false): mixed
```

Finds the last occurrence of a character in a string.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strrchr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strrchr.rs)).

**Examples**:

// Full example: examples/string-ops/main.php
echo strrchr("/var/log/app/error.log", "/");  // /error.log

## Internals

For how `strrchr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strrchr.md).
