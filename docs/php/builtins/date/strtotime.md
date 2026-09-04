---
title: "strtotime()"
description: "Parses an English textual datetime description into a Unix timestamp."
sidebar:
  order: 109
---

## strtotime()

```php
function strtotime(string $datetime, int $baseTimestamp = null): mixed
```

Parses an English textual datetime description into a Unix timestamp.

**Parameters**:
- `$datetime` (`string`)
- `$baseTimestamp` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/strtotime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/strtotime.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strtotime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/strtotime.md).
