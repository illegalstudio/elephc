const path = require('path');
const { runTests } = require('@vscode/test-electron');

async function main() {
  const extensionDevelopmentPath = path.resolve(__dirname, 'vscode-extension');
  const extensionTestsPath = path.resolve(__dirname, 'vscode-suite', 'index.js');
  const workspace = process.env.ELEPHC_SMOKE_DIR;
  if (!workspace) throw new Error('ELEPHC_SMOKE_DIR is required');
  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    extensionTestsEnv: { ...process.env },
    launchArgs: [
      workspace,
      '--disable-extensions',
      '--disable-gpu',
      '--no-sandbox',
      '--skip-welcome',
      '--skip-release-notes'
    ]
  });
}

main().catch(error => {
  console.error(error);
  process.exit(1);
});
