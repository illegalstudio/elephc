---
title: "strtr()"
description: "Translates bytes pairwise, or applies longest-match-first replacement pairs."
sidebar:
  order: 476
---

## strtr()

```php
function strtr(string $string, array|string $from, string $to = null): string
```

Translates bytes pairwise, or applies longest-match-first replacement pairs.

**Parameters**:
- `$string` (`string`)
- `$from` (`array|string`)
- `$to` (`string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strtr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strtr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strtr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strtr.md).
