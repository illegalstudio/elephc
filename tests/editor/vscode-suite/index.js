const assert = require('assert');
const fs = require('fs');
const path = require('path');
const vscode = require('vscode');

const TIMEOUT = 20000;

function waitFor(list, label) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${label}`)), TIMEOUT);
    list.push(value => {
      clearTimeout(timer);
      resolve(value);
    });
  });
}

async function run() {
  const folder = vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(folder, 'missing workspace folder');
  const workspace = folder.uri.fsPath;
  const mainPath = path.join(workspace, 'main.php');
  const uri = vscode.Uri.file(mainPath);
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);

  const breakpoint = new vscode.SourceBreakpoint(
    new vscode.Location(uri, new vscode.Position(5, 0))
  );
  vscode.debug.addBreakpoints([breakpoint]);

  const stoppedWaiters = [];
  const terminatedWaiters = [];
  const report = { stopped: [], frames: [], locals: [] };
  const tracker = vscode.debug.registerDebugAdapterTrackerFactory('elephc', {
    createDebugAdapterTracker() {
      return {
        onDidSendMessage(message) {
          if (message.type !== 'event') return;
          if (message.event === 'stopped') {
            report.stopped.push({ reason: message.body.reason, description: message.body.description });
            const waiter = stoppedWaiters.shift();
            if (waiter) waiter(message.body);
          }
          if (message.event === 'terminated') {
            const waiter = terminatedWaiters.shift();
            if (waiter) waiter();
          }
        }
      };
    }
  });

  try {
    // Register every waiter BEFORE the request that produces it: the adapter
    // can emit `stopped` before the DAP request response reaches the client.
    const firstStopped = waitFor(stoppedWaiters, 'first stopped');
    const started = await vscode.debug.startDebugging(folder, {
      type: 'elephc',
      request: 'launch',
      name: 'elephc VS Code smoke',
      program: mainPath,
      stopOnEntry: false
    });
    assert(started, 'startDebugging returned false');

    const first = await firstStopped;
    assert.strictEqual(first.reason, 'breakpoint');
    const session = vscode.debug.activeDebugSession;
    assert(session, 'missing active debug session');

    const stack1 = await session.customRequest('stackTrace', { threadId: first.threadId });
    report.frames.push({ when: 'breakpoint', frames: stack1.stackFrames.map(frame => ({ name: frame.name, line: frame.line })) });
    assert.strictEqual(stack1.stackFrames[0].name, 'run');
    assert.strictEqual(stack1.stackFrames[0].line, 6);
    const scopes1 = await session.customRequest('scopes', { frameId: stack1.stackFrames[0].id });
    const vars1 = await session.customRequest('variables', {
      variablesReference: scopes1.scopes[0].variablesReference
    });
    const x1 = vars1.variables.find(variable => variable.name === '$x');
    assert(x1, 'missing $x at breakpoint');
    report.locals.push({ when: 'breakpoint', name: '$x', value: x1.value });
    assert.strictEqual(x1.value, 'int(0)');

    const secondStopped = waitFor(stoppedWaiters, 'second stopped');
    await session.customRequest('next', { threadId: first.threadId });
    const second = await secondStopped;
    assert.strictEqual(second.reason, 'step');
    const stack2 = await session.customRequest('stackTrace', { threadId: second.threadId });
    report.frames.push({ when: 'step', frames: stack2.stackFrames.map(frame => ({ name: frame.name, line: frame.line })) });
    assert.strictEqual(stack2.stackFrames[0].name, 'run');
    assert.strictEqual(stack2.stackFrames[0].line, 7);
    const scopes2 = await session.customRequest('scopes', { frameId: stack2.stackFrames[0].id });
    const vars2 = await session.customRequest('variables', {
      variablesReference: scopes2.scopes[0].variablesReference
    });
    const x2 = vars2.variables.find(variable => variable.name === '$x');
    assert(x2, 'missing $x after step');
    report.locals.push({ when: 'step', name: '$x', value: x2.value });
    assert.strictEqual(x2.value, 'int(5)');

    const terminated = waitFor(terminatedWaiters, 'terminated');
    await session.customRequest('continue', { threadId: second.threadId });
    await terminated;
    fs.writeFileSync(
      path.join(workspace, 'vscode-smoke.json'),
      JSON.stringify(report, null, 2)
    );
    fs.writeFileSync(path.join(workspace, 'vscode-smoke.ok'), 'ok\n');
  } finally {
    vscode.debug.removeBreakpoints([breakpoint]);
    tracker.dispose();
  }
}

module.exports = { run };
