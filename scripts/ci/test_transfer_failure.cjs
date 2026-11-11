// Classify transport failures without allowing integrity or authentication retries.
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdtempSync, readFileSync, rmSync, writeFileSync } = require('node:fs');
const path = require('node:path');
const { test } = require('node:test');

const root = path.resolve(__dirname, '../..');
const modulePath = './.github/actions/download-test-archive/transfer_failure.cjs';

function classify(calls) {
  const script = `const f = require(${JSON.stringify(modulePath)});
    for (const [method, argument] of ${JSON.stringify(calls)}) f[method](argument);
    console.log(f.retryable());`;
  const result = spawnSync(process.execPath, ['-e', script], {
    cwd: root, encoding: 'utf8', timeout: 30000,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim() === 'true';
}

test('known network failures and an active-request deadline are recoverable', () => {
  for (const code of ['ECONNRESET', 'ETIMEDOUT', 'EAI_AGAIN', 'ENOTFOUND']) {
    assert.equal(classify([['requestStarted'], ['requestFailed', { code }]]), true, code);
  }
  for (const status of [408, 429, 500, 502, 503, 504]) {
    assert.equal(classify([['requestStarted'], ['responseReceived', status]]), true, status);
  }
  assert.equal(classify([['requestStarted'], ['deadlineExceeded']]), true);
});

test('a complete body, terminal status, disk error or missing request cannot recover', () => {
  assert.equal(classify([['deadlineExceeded']]), false);
  assert.equal(classify([['requestStarted'], ['bodyCompleted'], ['deadlineExceeded']]), false);
  assert.equal(classify([['requestStarted'], ['requestFailed', { code: 'ECONNRESET' }],
    ['requestStarted'], ['responseReceived', 200]]), false);
  for (const status of [401, 403, 404, 410, 422]) {
    assert.equal(classify([['requestStarted'], ['responseReceived', 503],
      ['responseReceived', status], ['deadlineExceeded']]), false, status);
  }
  for (const code of ['ENOSPC', 'EACCES', 'EINVAL', undefined]) {
    assert.equal(classify([['requestStarted'], ['requestFailed', { code }], ['deadlineExceeded']]), false);
  }
});

test('only a failed process publishes the bounded transfer flag', () => {
  const directory = mkdtempSync(path.join(root, '.transfer-classification.'));
  try {
    const output = path.join(directory, 'output');
    for (const status of [0, 1, 124]) {
      writeFileSync(output, '');
      const script = `const f = require(${JSON.stringify(modulePath)});
        f.requestStarted(); f.requestFailed({ code: 'ECONNRESET', message: 'secret-query' });
        process.exitCode = ${status};`;
      const result = spawnSync(process.execPath, ['-e', script], {
        cwd: root, env: { ...process.env, GITHUB_OUTPUT: output }, encoding: 'utf8', timeout: 30000,
      });
      assert.ifError(result.error);
      assert.equal(result.status, status, result.stderr);
      assert.equal(readFileSync(output, 'utf8'), status === 0 ? '' : 'retryable-transfer=true\n');
      assert.doesNotMatch(result.stdout + result.stderr, /secret-query/);
    }
  } finally {
    rmSync(directory, { recursive: true });
  }
});

test('the real HTTP observer distinguishes server failures, auth, complete and interrupted bodies', () => {
  const directory = mkdtempSync(path.join(root, '.transfer-classification.'));
  try {
    for (const [scenario, expected] of [['server', true], ['auth', false], ['complete', false], ['interrupt', true]]) {
      const output = path.join(directory, 'output');
      writeFileSync(output, '');
      const source = `
        import http from 'node:http';
        const scenario = ${JSON.stringify(scenario)};
        let outgoing;
        const server = http.createServer((req, res) => {
          outgoing = res;
          if (scenario === 'interrupt') {
            res.writeHead(200, { 'content-type': 'application/zip', 'content-length': 8 });
            res.write('part');
          } else {
            res.writeHead(scenario === 'server' ? 503 : (scenario === 'auth' ? 403 : 200),
              { 'content-type': 'application/zip', 'content-length': 8 });
            res.end('complete');
          }
        }).listen(0, '127.0.0.1');
        await new Promise(resolve => server.once('listening', resolve));
        const response = await new Promise(resolve => http.get(
          'http://127.0.0.1:' + server.address().port + '/', resolve));
        await new Promise(resolve => {
          response.once('end', resolve);
          response.once('error', resolve);
          if (scenario === 'interrupt') response.once('data', () => outgoing.destroy());
          response.resume();
        });
        server.close();
        process.exitCode = 1;
      `;
      const result = spawnSync(process.execPath, [
        '--require', './.github/actions/download-test-archive/deadline.cjs',
        '--input-type=module', '-e', source,
      ], { cwd: root, env: { ...process.env, GITHUB_OUTPUT: output }, encoding: 'utf8', timeout: 30000 });
      assert.ifError(result.error);
      assert.equal(result.status, 1, result.stderr);
      assert.equal(readFileSync(output, 'utf8'), expected ? 'retryable-transfer=true\n' : '', scenario);
    }
  } finally {
    rmSync(directory, { recursive: true });
  }
});
