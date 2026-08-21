---
title: "quotemeta()"
description: "Prefixes each regular-expression metacharacter in a string with a backslash."
sidebar:
  order: 448
---

## quotemeta()

```php
function quotemeta(string $string): string
```

Prefixes each regular-expression metacharacter in a string with a backslash.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/quotemeta.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/quotemeta.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `quotemeta` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/quotemeta.md).
