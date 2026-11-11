// Give the composite action a chance to retry a stalled or slowly trickling download.
// This preload affects only the official downloader process, including ZIP extraction.
// An unreferenced timer lets successful downloads exit immediately.
const { writeSync } = require('node:fs');
const progress = require('./progress.cjs');

setTimeout(() => {
  require('./transfer_failure.cjs').deadlineExceeded();
  progress.report();
  writeSync(2, '::error::Artifact download exceeded 5 minutes; failing this attempt so the composite action can retry.\n');
  process.exit(124);
}, 5 * 60 * 1000).unref();
