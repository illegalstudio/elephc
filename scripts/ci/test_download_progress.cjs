// Exercise the real preload against HTTP streams without downloading CI artifacts.
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const { test } = require('node:test');

const root = path.resolve(__dirname, '../..');
const preload = './.github/actions/download-test-archive/deadline.cjs';

function run(source, options = {}) {
  const result = spawnSync(process.execPath, [
    ...(options.preload === false ? [] : ['--require', preload]),
    '--input-type=module', '-e', source,
  ], { cwd: root, encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  return result;
}

test('progress preserves a paused response and every byte consumed by an ESM caller', () => {
  const result = run(`
    import assert from 'node:assert/strict';
    import http from 'node:http';
    import { createRequire } from 'node:module';
    import { setTimeout as delay } from 'node:timers/promises';
    const { report } = createRequire(import.meta.url)(
      './.github/actions/download-test-archive/progress.cjs');
    const payload = Buffer.alloc(128 * 1024, 65);
    const server = http.createServer((req, res) => {
      res.writeHead(200, { 'content-type': 'application/zip', 'content-length': payload.length,
        'x-private-header': 'secret-header' });
      res.end(payload);
    }).listen(0, '127.0.0.1');
    await new Promise(resolve => server.once('listening', resolve));
    const response = await new Promise(resolve => http.get(
      'http://127.0.0.1:' + server.address().port + '/artifact.zip?token=secret-query', resolve));
    // Wait with the response paused, as when the official downloader awaits a promise.
    await delay(60);
    assert.equal(response.readableFlowing, null);
    report();
    const chunks = [];
    response.on('data', chunk => chunks.push(chunk));
    await new Promise(resolve => response.once('end', resolve));
    assert.deepEqual(Buffer.concat(chunks), payload);
    server.close();
    console.log('all bytes preserved');
  `);
  assert.match(result.stdout, /all bytes preserved/);
  assert.match(result.stderr, /phase=download-and-zip-extraction[^\n]*received_bytes=0/);
  assert.match(result.stderr,
    /phase=zip-extraction-or-finalization[^\n]*received_bytes=131072 expected_bytes=131072/);
  assert.doesNotMatch(result.stderr, /secret-query|secret-header|127\.0\.0\.1|artifact\.zip/);
});

test('heartbeats report a stalled body before and after network completion', () => {
  const result = run(`
    import assert from 'node:assert/strict';
    import http from 'node:http';
    import { createRequire } from 'node:module';
    const interval = global.setInterval;
    let bodyStarted = false;
    let bodyFinished = false;
    let ticks = 0;
    let outgoing;
    let finish;
    const observed = new Promise(resolve => { finish = resolve; });
    global.setInterval = (callback, ms) => {
      assert.equal(ms, 15000);
      return interval(() => {
        callback();
        if (bodyFinished && ++ticks === 2) finish();
        else if (bodyStarted && !bodyFinished && ++ticks === 2) outgoing.end('done');
      }, 10);
    };
    createRequire(import.meta.url)('${preload}');
    const server = http.createServer((req, res) => {
      outgoing = res;
      res.writeHead(200, { 'content-type': 'application/zip', 'content-length': 8 });
      res.write('part');
    }).listen(0, '127.0.0.1');
    await new Promise(resolve => server.once('listening', resolve));
    const response = await new Promise(resolve => http.get(
      'http://127.0.0.1:' + server.address().port + '/', resolve));
    response.once('data', () => { bodyStarted = true; ticks = 0; });
    response.resume();
    await new Promise(resolve => response.once('end', resolve));
    // Release each phase only after its heartbeats, independent of runner speed.
    bodyFinished = true;
    ticks = 0;
    await observed;
    server.close();
  `, { preload: false });
  const reports = result.stderr.split('\n').filter(line => line.startsWith('Artifact progress:'));
  assert.ok(reports.filter(line => /received_bytes=4 expected_bytes=8/.test(line)).length >= 2,
    result.stderr);
  assert.ok(reports.filter(line => /phase=zip-extraction-or-finalization.*received_bytes=8/.test(line)).length >= 2,
    result.stderr);
});

test('an interrupted body retains the interruption and resets counters for a new transfer', () => {
  const result = run(`
    import assert from 'node:assert/strict';
    import http from 'node:http';
    import { createRequire } from 'node:module';
    const { report } = createRequire(import.meta.url)(
      './.github/actions/download-test-archive/progress.cjs');
    let count = 0;
    let incomplete;
    const server = http.createServer((req, res) => {
      res.writeHead(200, { 'content-type': 'application/zip', 'content-length': 8 });
      if (++count === 1) {
        incomplete = res;
        res.write('part');
      } else {
        res.end('complete');
      }
    }).listen(0, '127.0.0.1');
    await new Promise(resolve => server.once('listening', resolve));
    const address = 'http://127.0.0.1:' + server.address().port + '/';
    const first = await new Promise(resolve => http.get(address, resolve));
    first.once('data', () => incomplete.destroy());
    first.resume();
    await new Promise(resolve => first.once('error', error => {
      assert.equal(error.code, 'ECONNRESET');
      resolve();
    }));
    report();
    const second = await new Promise(resolve => http.get(address, resolve));
    second.resume();
    await new Promise(resolve => second.once('end', resolve));
    server.close();
  `);
  assert.match(result.stderr, /phase=transfer-interrupted[^\n]*transfer=1 received_bytes=4/);
  assert.match(result.stderr, /phase=download-and-zip-extraction[^\n]*transfer=2 received_bytes=0/);
  assert.match(result.stderr, /phase=zip-extraction-or-finalization[^\n]*transfer=2 received_bytes=8/);
});
