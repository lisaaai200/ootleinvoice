// Local preview server. Binds 127.0.0.1 only. No secrets, no chain access.
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import QRCode from 'qrcode';

const PORT = Number(process.env.PORT || 8787);
const HOST = process.env.HOST || '127.0.0.1';
const ROOT = join(import.meta.dirname, 'public');
const TYPES = { '.html': 'text/html; charset=utf-8', '.mjs': 'text/javascript', '.js': 'text/javascript', '.css': 'text/css' };
const SEC = {
  'Content-Security-Policy': "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; base-uri 'none'; form-action 'self'",
  'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer',
};

export const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url, `http://${req.headers.host || 'localhost'}`);
    if (url.pathname === '/api/qr') {
      const data = url.searchParams.get('data') || '';
      if (data.length > 1200 || !/^https?:\/\//.test(data)) { res.writeHead(400, SEC); return res.end('bad data'); }
      const svg = await QRCode.toString(data, { type: 'svg', errorCorrectionLevel: 'M', margin: 2 });
      res.writeHead(200, { ...SEC, 'Content-Type': 'image/svg+xml' }); return res.end(svg);
    }
    const rel = normalize(url.pathname === '/' ? '/index.html' : url.pathname).replace(/^(\.\.[/\\])+/, '');
    const file = join(ROOT, rel);
    if (!file.startsWith(ROOT)) { res.writeHead(403, SEC); return res.end(); }
    const body = await readFile(file);
    res.writeHead(200, { ...SEC, 'Content-Type': TYPES[extname(file)] || 'application/octet-stream' });
    res.end(body);
  } catch { res.writeHead(404, SEC); res.end('not found'); }
});
if (process.argv[1] === import.meta.filename) server.listen(PORT, HOST, () => console.log(`OotleInvoice preview: http://${HOST}:${PORT}/`));
