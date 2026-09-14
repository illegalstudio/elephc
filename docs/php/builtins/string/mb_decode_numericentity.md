---
title: "mb_decode_numericentity()"
description: "Decodes numeric entities selected by the ordered conversion map."
sidebar:
  order: 804
---

## mb_decode_numericentity()

```php
function mb_decode_numericentity(string $string, array $map, ?string $encoding = null): string
```

Decodes numeric entities selected by the ordered conversion map.

**Parameters**:
- `$string` (`string`)
- `$map` (`array`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_decode_numericentity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_decode_numericentity.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_decode_numericentity` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_decode_numericentity.md).
