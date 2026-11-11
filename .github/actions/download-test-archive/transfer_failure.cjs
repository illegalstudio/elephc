// Export only a bounded classification. Never persist URLs, headers or error text.
const { appendFileSync } = require('node:fs');
const transientCodes = new Set([
  'ECONNRESET', 'ETIMEDOUT', 'EAI_AGAIN', 'ENOTFOUND', 'ECONNREFUSED',
  'EHOSTUNREACH', 'ENETUNREACH', 'EPIPE', 'ERR_STREAM_PREMATURE_CLOSE',
  'UND_ERR_CONNECT_TIMEOUT', 'UND_ERR_HEADERS_TIMEOUT', 'UND_ERR_BODY_TIMEOUT', 'UND_ERR_SOCKET',
]);
const transientStatus = new Set([408, 429, 500, 502, 503, 504]);
let requested = false;
let transient = false;
let terminal = false;
let completed = false;
let currentRequest;
let pending = false;

function requestStarted(request) {
  requested = true;
  currentRequest = request;
  pending = true;
  // A recovered request must not classify a later, unrelated action error.
  transient = false;
}

function responseReceived(status, request) {
  if (request !== currentRequest) return;
  if (transientStatus.has(status)) transient = true;
  else if (status >= 400) terminal = true;
}

function requestFailed(error, request) {
  if (request !== currentRequest) return;
  pending = false;
  if (transientCodes.has(error?.code)) transient = true;
  else terminal = true;
}

function requestEnded(request) {
  if (request === currentRequest) pending = false;
}

function bodyCompleted() {
  // A later digest, ZIP, disk or action error must not become a network retry.
  completed = true;
}

function deadlineExceeded() {
  if (pending) transient = true;
}

function retryable() {
  return requested && transient && !terminal && !completed;
}

process.once('exit', code => {
  if (code !== 0 && retryable() && process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, 'retryable-transfer=true\n');
  }
});

module.exports = { requestStarted, responseReceived, requestFailed, requestEnded, bodyCompleted,
  deadlineExceeded, retryable };
