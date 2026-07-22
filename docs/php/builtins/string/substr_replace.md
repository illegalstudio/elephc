---
title: "substr_replace()"
description: "Replaces text within a portion of a string."
sidebar:
  order: 421
---

## substr_replace()

```php
function substr_replace(string $string, string $replace, int $offset, int $length = null): string
```

Replaces text within a portion of a string.

**Parameters**:
- `$string` (`string`)
- `$replace` (`string`)
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/substr_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/substr_replace.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `substr_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr_replace.md).
