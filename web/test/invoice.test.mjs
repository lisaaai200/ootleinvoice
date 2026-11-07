// Tests for the off-chain helper + DEMO simulation (not chain tests).
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseTari, formatTari, sha256Hex, buildPayUrl, parsePayUrl, escapeHtml, DemoLedger } from '../lib/invoice.mjs';

const memo = 'Logo design';
const base = async (o = {}) => ({ reference: 'INV-1', amount: 2_500_000n, memo, memoHash: await sha256Hex(memo), expiryEpoch: 10, ...o });

test('integer amount parsing', () => {
  assert.equal(parseTari('2.5'), 2_500_000n);
  assert.equal(parseTari('0.000001'), 1n);
  assert.equal(formatTari(2_500_000n), '2.5');
  for (const bad of ['0', '-1', '1.0000001', '1e3', 'abc', '', '0.1.2']) assert.throws(() => parseTari(bad), bad);
});
test('pay url round-trip carries only public terms', async () => {
  const inv = await base({ component: 'component_' + 'ab'.repeat(32) });
  const url = buildPayUrl('http://127.0.0.1:8787', inv);
  assert.doesNotMatch(url, /secret|key|seed/i);
  const back = await parsePayUrl(url);
  assert.equal(back.amount, inv.amount); assert.equal(back.memoHash, inv.memoHash);
});
test('tampered memo or amount in link rejected', async () => {
  const url = new URL(buildPayUrl('http://x', await base()));
  url.searchParams.set('m', 'other'); await assert.rejects(parsePayUrl(url.href), /memo does not match/);
  const u2 = new URL(buildPayUrl('http://x', await base())); u2.searchParams.set('a', '1.5');
  await assert.rejects(parsePayUrl(u2.href), /bad amount/);
});
test('invalid invoice fields rejected', async () => {
  for (const o of [{ reference: '<script>' }, { amount: 0n }, { memoHash: 'zz' }, { expiryEpoch: 0 }, { component: 'javascript:1' }])
    assert.throws(() => buildPayUrl('http://x', { reference: 'R', amount: 1n, memoHash: 'a'.repeat(64), expiryEpoch: 5, ...o }), JSON.stringify(o, (k, v) => typeof v === 'bigint' ? String(v) : v));
});
test('escapeHtml', () => assert.equal(escapeHtml('<a href="x">&\''), '&lt;a href=&quot;x&quot;&gt;&amp;&#39;'));
test('demo ledger mirrors template rules', async () => {
  const l = new DemoLedger(); const inv = await base();
  l.create(inv); assert.deepEqual(l.create(inv).reference, 'INV-1', 'idempotent');
  assert.throws(() => l.create({ ...inv, amount: 1n }), /different terms/);
  assert.throws(() => l.pay('INV-1', 2_499_999n), /exactly/);
  assert.throws(() => l.pay('INV-1', 2_500_001n), /exactly/);
  const r = l.pay('INV-1', 2_500_000n); assert.equal(r.simulated, true);
  assert.throws(() => l.pay('INV-1', 2_500_000n), /already paid/);
  assert.ok(l.verify(r)); assert.ok(!l.verify({ ...r, amount: '1' }));
  l.create(await base({ reference: 'INV-2', expiryEpoch: 3 })); l.state.epoch = 3; l.pay('INV-2', 2_500_000n);
  l.create(await base({ reference: 'INV-3', expiryEpoch: 4 })); l.state.epoch = 5;
  assert.equal(l.status('INV-3'), 'expired'); assert.throws(() => l.pay('INV-3', 2_500_000n), /expired/);
});
test('preview server serves pages, QR SVG, and blocks traversal', async () => {
  const { server } = await import('../server.mjs');
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const b = `http://127.0.0.1:${server.address().port}`;
  try {
    const idx = await fetch(b + '/'); assert.equal(idx.status, 200); assert.match(await idx.text(), /DEMO MODE/);
    assert.match(idx.headers.get('content-security-policy'), /script-src 'self'/);
    const qr = await fetch(b + '/api/qr?data=' + encodeURIComponent(b + '/pay.html?r=X')); assert.match(await qr.text(), /<svg/);
    assert.equal((await fetch(b + '/api/qr?data=javascript:alert(1)')).status, 400);
    assert.notEqual((await fetch(b + '/..%2f..%2fpackage.json')).status, 200);
  } finally { server.close(); }
});
test('browser copy of invoice.mjs is identical to lib', async () => {
  const { readFile } = await import('node:fs/promises');
  assert.equal(await readFile(new URL('../public/invoice.mjs', import.meta.url), 'utf8'), await readFile(new URL('../lib/invoice.mjs', import.meta.url), 'utf8'));
});
