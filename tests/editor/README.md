# Editor smoke tests

These harnesses drive the real `elephc debug --dap` adapter from an editor. They
are not part of `cargo test` because they need an editor runtime; CI can run them
in a separate job.

## nvim-dap

```bash
NVIM_BIN=/path/to/nvim \
NVIM_DAP_DIR=/path/to/nvim-dap \
NVIM_NIO_DIR=/path/to/nvim-nio \
./scripts/smoke-nvim-dap.sh
```

The script compiles its fixture, opens Neovim headless, sets a breakpoint, checks
the `run` frame and `$x`, steps once, checks `$x = int(5)`, continues, and expects
`SMOKE OK`.

## VS Code

```bash
VSCODE_TEST_ELECTRON_DIR=/path/to/node_modules \
./scripts/smoke-vscode.sh
```

`VSCODE_TEST_ELECTRON_DIR` is the directory containing `@vscode/test-electron`
(usually a temporary `npm install` prefix). Linux headless needs `xvfb-run`; the
first run downloads the VS Code test build. The suite writes `vscode-smoke.json`
with the stopped events, frames and `$x` values and fails on any mismatch.

The development extension in `vscode-extension/` contributes the `elephc`
debugger type and uses `adapter.js` as a transparent stdio proxy to
`elephc debug --dap`.
