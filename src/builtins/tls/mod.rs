//! Purpose:
//! Lists the internal `__elephc_tls_*` builtins used by the PHP 8.6 stream-context
//! prelude to install a TLS external-PSK callback.
//!
//! Called from:
//! - `crate::builtins` (registry collection via `inventory`).
//!
//! Key details:
//! - Both entries are `internal: true`: they are prelude plumbing, not PHP-visible
//!   functions, and they reference no TLS bridge symbol. The `ssl.psk_client_cb`
//!   source literal is what selects the libssl TLS stream bridge.

pub mod __elephc_tls_adapter_addr;
pub mod __elephc_tls_mark_early_data;
pub mod __elephc_tls_set_psk_client;
