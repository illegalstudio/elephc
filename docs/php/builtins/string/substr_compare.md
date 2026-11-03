---
title: "substr_compare()"
description: "Binary safe comparison of two strings from an offset, up to length characters."
sidebar:
  order: 1069
---

## substr_compare()

```php
function substr_compare(string $haystack, string $needle, int $offset, mixed $length = null, bool $case_insensitive = false): int
```

Binary safe comparison of two strings from an offset, up to length characters.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`)
- `$length` (`mixed`), default `null`, optional
- `$case_insensitive` (`bool`), default `false`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/substr_compare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/substr_compare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `substr_compare` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr_compare.md).
