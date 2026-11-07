// Shared invoice logic (browser + node). Amounts are BigInt micro-TARI; never floats.
export const MICRO = 1_000_000n;
const REF_RE = /^[A-Za-z0-9_-]{1,64}$/;
const HEX64 = /^[0-9a-f]{64}$/;
const ADDR_RE = /^component_[0-9a-f]{64}$/;

/** Parse "2.5" TARI -> 2500000n micro. Rejects >6 decimals, negatives, junk. */
export function parseTari(text) {
  const s = String(text).trim();
  const m = /^(\d{1,12})(?:\.(\d{1,6}))?$/.exec(s);
  if (!m) throw new Error('amount must be a positive number with at most 6 decimals');
  const v = BigInt(m[1]) * MICRO + BigInt((m[2] || '').padEnd(6, '0') || '0');
  if (v <= 0n) throw new Error('amount must be positive');
  return v;
}
export function formatTari(micro) {
  const v = BigInt(micro);
  const frac = (v % MICRO).toString().padStart(6, '0').replace(/0+$/, '');
  return `${v / MICRO}${frac ? '.' + frac : ''}`;
}
export async function sha256Hex(text) {
  const data = new TextEncoder().encode(text);
  const buf = await globalThis.crypto.subtle.digest('SHA-256', data);
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
}
export function validateInvoice(inv) {
  const errs = [];
  if (!REF_RE.test(inv.reference ?? '')) errs.push('reference must be 1-64 chars of [A-Za-z0-9_-]');
  if (typeof inv.amount !== 'bigint' || inv.amount <= 0n) errs.push('amount must be a positive integer (micro-TARI)');
  if (!HEX64.test(inv.memoHash ?? '')) errs.push('memoHash must be 64 lowercase hex chars');
  if (!Number.isSafeInteger(inv.expiryEpoch) || inv.expiryEpoch < 1) errs.push('expiryEpoch must be a positive integer');
  if (inv.component != null && !ADDR_RE.test(inv.component)) errs.push('component must look like component_<64 hex>');
  if (typeof inv.memo === 'string' && inv.memo.length > 280) errs.push('memo max 280 chars');
  if (errs.length) throw new Error(errs.join('; '));
  return inv;
}
/** Invoice link. Contains only public invoice terms — no keys, no secrets. */
export function buildPayUrl(base, inv) {
  validateInvoice(inv);
  const u = new URL('/pay.html', base);
  const p = u.searchParams;
  if (inv.component) p.set('c', inv.component);
  p.set('r', inv.reference); p.set('a', inv.amount.toString()); p.set('h', inv.memoHash);
  p.set('e', String(inv.expiryEpoch));
  if (inv.memo) p.set('m', inv.memo);
  if (inv.demo) p.set('demo', '1');
  return u.toString();
}
export async function parsePayUrl(href) {
  const p = new URL(href).searchParams;
  const a = p.get('a') ?? '';
  if (!/^\d{1,20}$/.test(a)) throw new Error('bad amount in link');
  const inv = {
    component: p.get('c') || null, reference: p.get('r') ?? '', amount: BigInt(a),
    memoHash: p.get('h') ?? '', expiryEpoch: Number(p.get('e')), memo: p.get('m') ?? '', demo: p.get('demo') === '1',
  };
  validateInvoice(inv);
  if (inv.memo && (await sha256Hex(inv.memo)) !== inv.memoHash) throw new Error('memo does not match memo hash (link tampered?)');
  return inv;
}
export function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

/**
 * DEMO SIMULATION ONLY. Mirrors the on-chain template's rules in memory so the UI can be
 * tried without a wallet. It is NOT connected to Tari and proves nothing on-chain.
 */
export class DemoLedger {
  constructor(state = { epoch: 1, invoices: {} }) { this.state = state; }
  create(inv) {
    validateInvoice(inv);
    const ex = this.state.invoices[inv.reference];
    if (ex) {
      if (ex.amount !== inv.amount.toString() || ex.memoHash !== inv.memoHash || ex.expiryEpoch !== inv.expiryEpoch)
        throw new Error('reference already used with different terms');
      return ex;
    }
    if (inv.expiryEpoch <= this.state.epoch) throw new Error('expiry must be in the future');
    const rec = { reference: inv.reference, amount: inv.amount.toString(), memoHash: inv.memoHash, expiryEpoch: inv.expiryEpoch, status: 'open' };
    this.state.invoices[inv.reference] = rec;
    return rec;
  }
  status(ref) {
    const r = this.state.invoices[ref];
    if (!r) throw new Error('unknown invoice');
    if (r.status === 'open' && this.state.epoch > r.expiryEpoch) return 'expired';
    return r.status;
  }
  pay(ref, amount) {
    const r = this.state.invoices[ref];
    if (!r) throw new Error('unknown invoice');
    if (r.status !== 'open') throw new Error('invoice already paid');
    if (this.state.epoch > r.expiryEpoch) throw new Error('invoice expired');
    if (BigInt(amount) !== BigInt(r.amount)) throw new Error('payment amount must equal invoice amount exactly');
    r.status = 'paid'; r.paidEpoch = this.state.epoch;
    return { simulated: true, reference: ref, amount: r.amount, memoHash: r.memoHash, paidEpoch: r.paidEpoch };
  }
  verify(receipt) {
    const r = this.state.invoices[receipt.reference];
    return !!r && r.status === 'paid' && r.amount === String(receipt.amount) && r.memoHash === receipt.memoHash;
  }
}
