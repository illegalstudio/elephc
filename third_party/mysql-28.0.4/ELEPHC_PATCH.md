# elephc patch of `mysql` 28.0.4

This is the crates.io `mysql` 28.0.4 source, wired in through `[patch.crates-io]` in the
workspace `Cargo.toml`, with one addition in `src/conn/mod.rs` (marked `elephc patch`):

- `Conn::status_flags()` — the server status flags of the last OK/EOF packet, which the
  upstream crate tracks but does not expose (mysqli's `MYSQLI_REPORT_INDEX`).
- `Conn::refresh(options)` — `COM_REFRESH` (mysqli's `mysqli::refresh()`).
- `Conn::dump_debug_info()` — `COM_DEBUG` (mysqli's `mysqli::dump_debug_info()`).

Everything else is byte-for-byte upstream (MIT / Apache-2.0, see the license files). To
upgrade, copy the new release over this directory and re-apply the three methods.
