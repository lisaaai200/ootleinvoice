import { parseTari, sha256Hex, buildPayUrl, DemoLedger } from './invoice.mjs';
import QRCode from './qrcode.mjs';
const $ = (id) => document.getElementById(id);
const load = () => new DemoLedger(JSON.parse(localStorage.getItem('oi-demo') || '{"epoch":1,"invoices":{}}'));
const appBase = () => new URL('.', location.href).href;
$('f').addEventListener('submit', async (ev) => {
  ev.preventDefault(); $('msg').textContent = ''; $('msg').className = '';
  try {
    const memo = $('memo').value;
    const inv = { reference: $('ref').value.trim(), amount: parseTari($('amt').value), memo,
      memoHash: await sha256Hex(memo), expiryEpoch: Number($('exp').value), component: $('comp').value.trim() || null, demo: true };
    const l = load(); l.create(inv); localStorage.setItem('oi-demo', JSON.stringify(l.state));
    const url = buildPayUrl(appBase(), inv);
    $('link').textContent = url;
    try { $('qr').src = await QRCode.toDataURL(url, { margin: 2, errorCorrectionLevel: 'M', width: 240 }); }
    catch { $('qr').removeAttribute('src'); }
    $('out').hidden = false;
    $('msg').textContent = 'Invoice created (simulated).'; $('msg').className = 'ok';
  } catch (e) { $('msg').textContent = e.message; $('msg').className = 'err'; }
});
