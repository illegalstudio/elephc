---
title: "strcasecmp()"
description: "Binary safe case-insensitive string comparison. Returns negative, zero, or positive."
sidebar:
  order: 417
---

## strcasecmp()

```php
function strcasecmp(string $string1, string $string2): int
```

Binary safe case-insensitive string comparison. Returns negative, zero, or positive.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strcasecmp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strcasecmp.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strcasecmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcasecmp.md).
