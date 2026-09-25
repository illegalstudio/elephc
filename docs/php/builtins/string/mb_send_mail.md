---
title: "mb_send_mail()"
description: "Encodes the subject and body with the active language settings and sends the message through the configured mail transport."
sidebar:
  order: 868
---

## mb_send_mail()

```php
function mb_send_mail(string $to, string $subject, string $message, array|string $additional_headers = [], ?string $additional_params = null): bool
```

Encodes the subject and body with the active language settings and sends the message through the configured mail transport.

**Parameters**:
- `$to` (`string`)
- `$subject` (`string`)
- `$message` (`string`)
- `$additional_headers` (`array|string`), default `[]`, optional
- `$additional_params` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_send_mail.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_send_mail.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_send_mail` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_send_mail.md).
