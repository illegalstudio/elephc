// Observe the official downloader without adding a data listener, which would
// start the response flowing before the downloader attaches its extraction pipe.
// Never log request URLs, headers or errors: artifact URLs contain credentials.
const { subscribe } = require('node:diagnostics_channel');
const { writeSync } = require('node:fs');

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
  const host = request.host || '';
  const blob = host.endsWith('.blob.core.windows.net') ||
    host.endsWith('.results.githubusercontent.com');
  if (blob) {
    phase = 'waiting-for-blob-response';
    report();
  }
  request.prependOnceListener('response', response => {
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
          phase = 'zip-extraction-or-finalization';
          report();
        } else if (event === 'aborted' || event === 'error') {
          phase = 'transfer-interrupted';
          report();
        }
      }
      return Reflect.apply(emit, this, [event, ...args]);
    };
  });
});

// Both diagnostics timers are unreferenced, so successful actions exit naturally.
setInterval(report, 15 * 1000).unref();
module.exports = { report };
