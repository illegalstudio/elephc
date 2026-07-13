---
title: "explode()"
description: "Splits a string by a separator into an array of substrings."
sidebar:
  order: 350
---

## explode()

```php
function explode(string $separator, string $string, int $limit = PHP_INT_MAX): array
```

Splits a string by a separator into an array of substrings.

**Parameters**:
- `$separator` (`string`)
- `$string` (`string`)
- `$limit` (`int`), default `PHP_INT_MAX`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/explode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/explode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `explode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/explode.md).

