---
title: "substr()"
description: "Returns a portion of a string specified by the offset and length."
sidebar:
  order: 418
---

## substr()

```php
function substr(string $string, int $offset, int $length = null): string
```

Returns a portion of a string specified by the offset and length.

**Parameters**:
- `$string` (`string`)
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/substr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/substr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr.md).

