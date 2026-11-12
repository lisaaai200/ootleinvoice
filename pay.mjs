import { parsePayUrl, formatTari, escapeHtml, DemoLedger } from './invoice.mjs';
const $ = (id) => document.getElementById(id);
const load = () => new DemoLedger(JSON.parse(localStorage.getItem('oi-demo') || '{"epoch":1,"invoices":{}}'));
const save = (l) => localStorage.setItem('oi-demo', JSON.stringify(l.state));
const show = (t, ok) => { $('msg').textContent = t; $('msg').className = ok ? 'ok' : 'err'; };
let inv, receipt;
function render() {
  const l = load(); let st = 'not found in this browser demo ledger';
  try { st = l.status(inv.reference); } catch {}
  $('inv').innerHTML = `<p><b>Ref:</b> ${escapeHtml(inv.reference)}<br><b>Amount:</b> ${escapeHtml(formatTari(inv.amount))} TARI (${escapeHtml(inv.amount)} µT)<br>
  <b>Memo:</b> ${escapeHtml(inv.memo)}<br><b>Memo hash:</b> <code>${escapeHtml(inv.memoHash)}</code><br><b>Expires after epoch:</b> ${escapeHtml(inv.expiryEpoch)} (demo epoch now ${escapeHtml(l.state.epoch)})<br>
  <b>Component:</b> ${inv.component ? `<code>${escapeHtml(inv.component)}</code>` : '(demo)'}<br><b>Status:</b> ${escapeHtml(st)}</p>`;
}
try { inv = await parsePayUrl(location.href); $('send').value = inv.amount.toString(); render(); }
catch (e) { show(e.message, false); $('pay').disabled = true; }
$('pay').onclick = () => {
  try { const l = load(); receipt = l.pay(inv.reference, BigInt($('send').value.trim() || '0')); save(l);
    $('receipt').textContent = 'SIMULATED RECEIPT\n' + JSON.stringify(receipt, null, 2); $('verify').hidden = false; show('Paid (simulated).', true);
  } catch (e) { show(e.message, false); } render();
};
$('tick').onclick = () => { const l = load(); l.state.epoch += 1; save(l); render(); };
$('verify').onclick = () => show(load().verify(receipt) ? 'Receipt matches demo ledger.' : 'Receipt does NOT verify.', load().verify(receipt));
