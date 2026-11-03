# elephc patch of `mysql` 28.0.4

This is the crates.io `mysql` 28.0.4 source, wired in through `[patch.crates-io]` in the
workspace `Cargo.toml`, with these additions in `src/conn/mod.rs` (marked `elephc patch`):

- `Conn::status_flags()` — the server status flags of the last OK/EOF packet, which the
  upstream crate tracks but does not expose (mysqli's `MYSQLI_REPORT_INDEX`).
- `Conn::refresh(options)` — `COM_REFRESH` (mysqli's `mysqli::refresh()`).
- `Conn::dump_debug_info()` — `COM_DEBUG` (mysqli's `mysqli::dump_debug_info()`).
- `Conn::set_server_option(option)` — `COM_SET_OPTION` (mysqli's per-`multi_query()`
  multi-statement toggle), and in `src/conn/opts/mod.rs` the `multi_statements` option
  (`OptsBuilder::multi_statements`, default `true`) that lets a mysqli handshake omit
  `CLIENT_MULTI_STATEMENTS`.
- `IoCounters`, `GLOBAL_IO_COUNTERS`, and `Conn::io_counters()` — byte and packet counters
  updated in `raw_read_packet` / `write_packet`, the two points every packet passes
  (mysqli's `get_connection_stats()` / `mysqli_get_client_stats()`).

Everything else is byte-for-byte upstream (MIT / Apache-2.0, see the license files). To
upgrade, copy the new release over this directory and re-apply the additions.
