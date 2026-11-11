// Observe the official downloader without adding a data listener, which would
// start the response flowing before the downloader attaches its extraction pipe.
// Never log request URLs, headers or errors: artifact URLs contain credentials.
const { subscribe } = require('node:diagnostics_channel');
const { writeSync } = require('node:fs');
const failure = require('./transfer_failure.cjs');

const started = Date.now();
let phase = 'artifact-api';
let transfer;
let transfers = 0;

function report() {
  const seconds = Math.floor((Date.now() - started) / 1000);
  let details = '';
  if (transfer) {
    const idle = Math.floor((Date.now() - transfer.lastByte) / 1000);
    details = ` transfer=${transfer.number} received_bytes=${transfer.bytes}` +
      ` expected_bytes=${transfer.expected} buffered_bytes=${transfer.response.readableLength}` +
      ` idle_seconds=${idle}`;
  }
  writeSync(2, `Artifact progress: phase=${phase} elapsed_seconds=${seconds}${details}\n`);
}

subscribe('http.client.request.start', ({ request }) => {
  failure.requestStarted(request);
  const host = request.host || '';
  const blob = host.endsWith('.blob.core.windows.net') ||
    host.endsWith('.results.githubusercontent.com');
  if (blob) {
    phase = 'waiting-for-blob-response';
    report();
  }
  request.prependOnceListener('response', response => {
    failure.responseReceived(response.statusCode, request);
    response.prependOnceListener('end', () => failure.requestEnded(request));
    const mime = (response.headers['content-type'] || '').split(';', 1)[0].trim();
    if (response.statusCode !== 200 || !(blob || mime === 'application/zip' ||
        mime === 'application/x-zip-compressed' || /\.zip(?:\?|$)/i.test(request.path))) {
      return;
    }
    const length = response.headers['content-length'];
    const current = {
      number: ++transfers,
      bytes: 0,
      expected: /^\d+$/.test(length || '') ? length : 'unknown',
      lastByte: Date.now(),
      response,
    };
    transfer = current;
    phase = 'download-and-zip-extraction';
    report();
    const emit = response.emit;
    response.emit = function (event, ...args) {
      if (event === 'data') {
        current.bytes += Buffer.byteLength(args[0]);
        current.lastByte = Date.now();
      }
      if (transfer === current) {
        if (event === 'end') {
          failure.bodyCompleted();
          phase = 'zip-extraction-or-finalization';
          report();
        } else if (event === 'aborted' || event === 'error') {
          if (event === 'error') failure.requestFailed(args[0], request);
          phase = 'transfer-interrupted';
          report();
        }
      }
      return Reflect.apply(emit, this, [event, ...args]);
    };
  });
});

subscribe('http.client.request.error', ({ request, error }) => failure.requestFailed(error, request));

// Octokit's artifact API uses fetch, whose requests bypass Node's http channels.
// Observe status codes without reading API bodies or logging signed URLs.
const fetchArchives = new WeakSet();
subscribe('undici:request:create', ({ request }) => failure.requestStarted(request));
subscribe('undici:request:headers', ({ request, response }) => {
  failure.responseReceived(response.statusCode, request);
  if (response.statusCode !== 200) return;
  const host = new URL(request.origin).hostname;
  let mime = '';
  for (let index = 0; index < response.headers.length; index += 2) {
    if (String(response.headers[index]).toLowerCase() === 'content-type') {
      mime = String(response.headers[index + 1]).split(';', 1)[0].trim();
      break;
    }
  }
  if (host.endsWith('.blob.core.windows.net') || host.endsWith('.results.githubusercontent.com') ||
      mime === 'application/zip' || mime === 'application/x-zip-compressed' ||
      /\.zip(?:\?|$)/i.test(request.path)) {
    fetchArchives.add(request);
  }
});
subscribe('undici:request:trailers', ({ request }) => {
  if (fetchArchives.has(request)) failure.bodyCompleted();
  failure.requestEnded(request);
});
subscribe('undici:request:error', ({ request, error }) => failure.requestFailed(error, request));

// Both diagnostics timers are unreferenced, so successful actions exit naturally.
setInterval(report, 15 * 1000).unref();
module.exports = { report };
