// Browser-independent checks for the shipped pairing page. No dependencies or
// network access; fetch/DOM/history are bounded test doubles.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const html = fs.readFileSync(path.join(__dirname, '../rust/src/apple_transfer.html'), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const secret = 'a'.repeat(32);

function page(hash = '', pairStatus = 200) {
  const elements = {};
  const element = id => elements[id] ||= { value: '', disabled: ['upload', 'cancel'].includes(id), textContent: '', files: [] };
  const calls = [];
  const history = [];
  const timers = [];
  const uploads = [];
  const state = { status: 200, busy: false };
  const done = vm.runInNewContext(script, {
    document: { getElementById: element }, location: { hash, pathname: '/' },
    history: { replaceState: (...args) => history.push(args) }, setInterval: callback => timers.push(callback),
    fetch: async (url, options) => {
      calls.push({ url, options });
      if (url === '/pair') return { ok: pairStatus === 200, status: pairStatus,
        json: async () => ({ token: secret }), text: async () => 'Code rejected or expired' };
      if (url === '/cancel') { state.busy = false; return { ok: true }; }
      assert.equal(url, '/status');
      if (state.status !== 200) return { ok: false, status: state.status, text: async () => 'Pair again with the TV code' };
      return { ok: true, status: 200, json: async () => ({ message: 'Ready', busy: state.busy, total: 0, received: 0 }) };
    },
    XMLHttpRequest: function() {
      this.upload = {};
      this.open = (method, url) => { this.method = method; this.url = url; };
      this.setRequestHeader = (name, value) => { this[name] = value; };
      this.send = file => { this.file = file; uploads.push(this); };
    },
  });
  return { elements, calls, history, done, timers, state, uploads };
}

(async () => {
  const manual = page();
  manual.elements.code.value = '012 345';
  await manual.elements.pair.onclick();
  assert.equal(manual.calls[0].url, '/pair');
  assert.equal(manual.calls[0].options.body, '012345');
  assert.equal(manual.calls[1].options.headers.Authorization, 'Bearer ' + secret);
  assert.equal(manual.elements.code.value, '');
  assert.equal(manual.elements.upload.disabled, true); // A valid file is also required.
  assert.equal(manual.elements.connection.textContent, 'Connected');
  for (const file of [{ name: 'kernel.img', size: 100 }, { name: 'empty.hsqs', size: 0 },
    { name: 'large.hsqs', size: 1024 ** 3 + 1 }]) {
    manual.elements.file.files = [file]; manual.elements.file.onchange();
    assert.equal(manual.elements.upload.disabled, true);
    manual.elements.upload.onclick(); assert.equal(manual.uploads.length, 0);
  }
  manual.elements.file.files = [{ name: '<collection>.HSQS', size: 1024 ** 2 }];
  manual.elements.file.onchange();
  assert.equal(manual.elements.upload.disabled, false);
  assert.match(manual.elements['file-info'].textContent, /<collection>.HSQS · 1.0 MiB/);
  manual.elements.upload.onclick();
  assert.equal(manual.uploads.length, 1);
  assert.equal(manual.uploads[0].Authorization, 'Bearer ' + secret);
  assert.equal(manual.elements.file.disabled, true);
  manual.uploads[0].upload.onprogress({ lengthComputable: true, loaded: 5, total: 10 });
  assert.match(manual.elements.status.textContent, /50%/);
  manual.state.busy = true; await manual.timers[0]();
  assert.match(manual.elements.status.textContent, /50%/); // Polls must not overwrite upload progress.
  assert.equal(manual.elements.cancel.disabled, false);
  await manual.elements.cancel.onclick();
  assert.equal(manual.calls.at(-2).url, '/cancel');
  manual.uploads[0].onerror();
  assert.equal(manual.elements.upload.disabled, true);
  assert.match(manual.elements.status.textContent, /Connection lost/);
  await manual.timers[0]();
  assert.equal(manual.elements.upload.disabled, false);
  manual.state.status = 403;
  manual.state.busy = true;
  await manual.timers[0]();
  assert.equal(manual.elements.upload.disabled, true);
  assert.equal(manual.elements.cancel.disabled, true);
  assert.equal(manual.elements.pair.disabled, false);

  const qr = page('#012345');
  await qr.done;
  assert.equal(qr.calls[0].options.body, '012345');
  assert.equal(qr.history[0][2], '/');

  for (const status of [403, 410, 429]) {
    const rejected = page('#012345', status);
    await rejected.done;
    assert.equal(rejected.calls.length, 1);
    assert.equal(rejected.elements.upload.disabled, true);
    assert.equal(rejected.elements.pair.disabled, false);
    assert.match(rejected.elements.status.textContent, /rejected/);
  }
  for (const code of ['1234', 'abcdef', '１２３４５６']) {
    const invalid = page(); invalid.elements.code.value = code;
    await invalid.elements.pair.onclick();
    assert.equal(invalid.calls.length, 0);
  }
  console.log('Transfer page: pairing/revocation, file validation, upload progress, cancellation and reconnect checks passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
