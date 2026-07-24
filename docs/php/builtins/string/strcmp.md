---
title: "strcmp()"
description: "Binary safe string comparison. Returns negative, zero, or positive."
sidebar:
  order: 418
---

## strcmp()

```php
function strcmp(string $string1, string $string2): int
```

Binary safe string comparison. Returns negative, zero, or positive.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strcmp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strcmp.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strcmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcmp.md).
