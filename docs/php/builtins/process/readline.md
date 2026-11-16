---
title: "readline()"
description: "Reads a line from the user's terminal."
---

## readline()

```php
function readline(?string $prompt = null): mixed
```

Reads a line from the user's terminal.

**Parameters**:
- `$prompt` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `readline` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/readline.md).
