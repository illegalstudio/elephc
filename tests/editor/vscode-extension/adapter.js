#!/usr/bin/env node
const { spawn } = require('child_process');

const elephc = process.env.ELEPHC_BIN;
const program = process.env.ELEPHC_SMOKE_PROGRAM;
const cwd = process.env.ELEPHC_SMOKE_DIR;
if (!elephc || !program || !cwd) {
  console.error('elephc debug adapter proxy needs ELEPHC_BIN, ELEPHC_SMOKE_PROGRAM and ELEPHC_SMOKE_DIR');
  process.exit(1);
}

const child = spawn(elephc, ['debug', '--dap', program], {
  cwd,
  stdio: ['pipe', 'pipe', 'inherit'],
});
child.on('error', error => {
  console.error('cannot start elephc debug adapter:', error.message);
  process.exit(1);
});
child.on('exit', code => process.exit(code === null ? 1 : code));
process.stdin.pipe(child.stdin);
child.stdout.pipe(process.stdout);
