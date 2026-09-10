---
title: "mb_encode_numericentity()"
description: "Encodes mapped characters as decimal or hexadecimal numeric entities."
sidebar:
  order: 808
---

## mb_encode_numericentity()

```php
function mb_encode_numericentity(string $string, array $map, ?string $encoding = null, bool $hex = false): string
```

Encodes mapped characters as decimal or hexadecimal numeric entities.

**Parameters**:
- `$string` (`string`)
- `$map` (`array`)
- `$encoding` (`?string`), default `null`, optional
- `$hex` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_encode_numericentity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_encode_numericentity.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_encode_numericentity` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_encode_numericentity.md).
