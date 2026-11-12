# OotleInvoice

Pay-by-link invoices with verifiable receipts for **Tari Ootle**: a freelancer or small
merchant creates an exact-amount invoice, shares a link/QR, the payer pays in one atomic
transaction, funds go straight to the merchant's account, and the payer gets a receipt NFT.

> **Status: non-production prototype.** Not audited. Not deployed to any network.
> No security guarantees beyond what the tests below demonstrate.


## Live demo

A walletless browser simulation is published at
**https://lisaaai200.github.io/ootleinvoice/** . It is the same DEMO MODE as `web/`:
no wallet, no Tari network, and no real payments. Mainnet (and testnet) deployment
of the Ootle template is pending and is not included in this release.

## What is real vs. simulated

| Part | Level of integration |
|---|---|
| `template/` Rust `#[template]` (`tari_template_lib 0.38`) | **Real Ootle template.** Compiles to `wasm32-unknown-unknown`. |
| `template/tests/` | **Executed in the official Tari Ootle engine in-process** via `tari_template_test_tooling 0.50` (real WASM execution, real builtin Account/faucet templates, real access rules, synthetic local test accounts — no network, no wallet, no funds). |
| `web/` UI | **DEMO MODE — browser-only simulation** (`DemoLedger`) that mirrors the template rules. It does not talk to Tari; its "receipts" prove nothing on-chain. Clearly labelled in the UI. |
| Testnet / mainnet deployment, wallet payment flow | **Not done.** Requires a wallet, publishing fees and network access. |

## Template API (`OotleInvoice`)

- `new(payee: ComponentAddress)` — invoice book owned by the signer; payments forward to `payee`.
- `create_invoice(reference, amount, memo_hash, expiry_epoch) -> u64` — owner only. Integer
  micro-TARI `Amount`; `memo_hash` = SHA-256 hex of the off-chain memo; expiry in Ootle epochs.
  Idempotent per `reference` (same terms return same id; different terms rejected).
- `pay(invoice_id, payment: Bucket) -> Bucket` — anyone. Requires TARI, **exact** amount,
  open and not past expiry. Forwards funds to payee (no custody) and returns receipt NFT
  (id = invoice id), mintable only by this component.
- `get_invoice(id) -> (amount, memo_hash, expiry_epoch, status)` (0 open, 1 paid, 2 expired).
- `verify_receipt(id, amount, memo_hash) -> bool`.

Any failure panics, so the whole transaction (including the payer's withdrawal) reverts.

Out of scope: custody/escrow, refunds, partial payments, subscriptions, fiat, KYC.

## Run

```bash
# template (needs rustup stable + wasm32-unknown-unknown target)
cd template
cargo build --target wasm32-unknown-unknown --release
cargo test --release          # first run compiles the engine (~2 min on 8 cores)

# web demo
cd ../web && npm install && npm test
PORT=8795 npm start           # http://127.0.0.1:8795/ (binds localhost only)
```

Verified locally: `tari_template_lib` 0.38.0, `tari_template_test_tooling` 0.50.1, release WASM ~158 KB (unoptimised). Run `wasm-opt` before any network publish.

## Tests (engine harness)

exact payment forwards funds + issues receipt + verifies; wrong amount (±1 µT) rejected with
payer balance unchanged; duplicate payment rejected, payee paid once; create idempotent per
reference; expired invoice unpayable (reported expired); payment on expiry epoch accepted;
invalid inputs rejected; non-owner cannot create invoices.

## Security notes / limits

- No secrets in links: pay URLs carry only public invoice terms; the page re-checks the memo
  against its hash to detect tampering. Untrusted link data is validated and HTML-escaped; strict CSP.
- Receipt NFT proves payment only together with `verify_receipt` on the genuine component address.
- Invoice book capped at 1000 invoices (state growth); no cleanup yet.
- Epoch-based expiry is coarse. WASM is ~158 KB unoptimised; run `wasm-opt` before any publish.
