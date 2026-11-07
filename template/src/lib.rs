//! OotleInvoice — exact-amount invoices with non-fungible receipts on Tari Ootle.
//!
//! NON-PRODUCTION PROTOTYPE. Not audited. Tested only in-process with the official
//! `tari_template_test_tooling` engine harness (see README).
//!
//! Design notes:
//! * No custody: a correct payment is forwarded to the merchant's payee account in the
//!   same transaction. The component never holds payment funds.
//! * Amounts are integer micro-units (`Amount`), never floats.
//! * Only a 32-byte memo hash is stored on-chain; the human-readable memo stays off-chain.
//! * Expiry is expressed in Ootle epochs (`Consensus::current_epoch()`).
//! * Failures panic, so the whole transaction (incl. the payer's withdrawal) reverts.
use tari_template_lib::prelude::*;

#[template]
mod ootle_invoice {
    use super::*;
    use std::collections::BTreeMap;

    const MAX_OPEN_INVOICES: usize = 1000;

    pub struct OotleInvoice {
        payee: ComponentAddress,
        receipt_resource: ResourceAddress,
        next_id: u64,
        invoices: BTreeMap<u64, Invoice>,
        /// merchant reference -> invoice id; makes create_invoice idempotent per reference
        references: BTreeMap<String, u64>,
    }

    impl OotleInvoice {
        /// Create an invoice book. `payee` is the account that receives payments.
        pub fn new(payee: ComponentAddress) -> Component<Self> {
            let alloc = CallerContext::allocate_component_address(None);
            let self_address = alloc.get_address();
            let receipt_resource = ResourceBuilder::non_fungible()
                .with_token_symbol("OIRCPT")
                // only this invoice component may mint receipts
                .mintable(rule!(component(self_address)), UpdateRule::Locked)
                .metadata("name", "OotleInvoice receipt")
                .build();
            Component::new(Self {
                payee,
                receipt_resource,
                next_id: 1,
                invoices: BTreeMap::new(),
                references: BTreeMap::new(),
            })
            .with_access_rules(
                ComponentAccessRules::new()
                    .method("pay", rule!(allow_all))
                    .method("get_invoice", rule!(allow_all))
                    .method("verify_receipt", rule!(allow_all))
                    .method("receipt_resource", rule!(allow_all))
                    .method("payee", rule!(allow_all))
                    .default(rule!(deny_all)),
            )
            .with_address_allocation(alloc)
            .with_owner_rule(OwnerRule::OwnedBySigner)
            .create()
        }

        /// Owner only. Creates an invoice for an exact `amount` of TARI micro-units.
        /// Calling again with the same `reference` returns the existing id (idempotent)
        /// provided the terms match; mismatching terms are rejected.
        pub fn create_invoice(&mut self, reference: String, amount: Amount, memo_hash: String, expiry_epoch: u64) -> u64 {
            assert!(!reference.is_empty() && reference.len() <= 64, "reference must be 1-64 chars");
            assert!(
                reference.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                "reference must be [A-Za-z0-9_-]"
            );
            assert!(amount.is_positive(), "amount must be positive");
            assert!(
                memo_hash.len() == 64 && memo_hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "memo_hash must be 64 lowercase hex chars"
            );
            if let Some(id) = self.references.get(&reference) {
                let inv = self.invoices.get(id).expect("index corrupt");
                assert!(
                    inv.amount == amount && inv.memo_hash == memo_hash && inv.expiry_epoch == expiry_epoch,
                    "reference already used with different terms"
                );
                return *id;
            }
            assert!(expiry_epoch > Consensus::current_epoch(), "expiry must be in the future");
            assert!(self.invoices.len() < MAX_OPEN_INVOICES, "invoice book full");
            let id = self.next_id;
            self.next_id += 1;
            self.invoices.insert(id, Invoice {
                reference: reference.clone(),
                amount,
                memo_hash,
                expiry_epoch,
                status: InvoiceStatus::Open,
                paid_epoch: 0,
            });
            self.references.insert(reference, id);
            emit_event("InvoiceCreated", metadata!["invoice_id" => id.to_string(), "amount" => amount.to_string()]);
            id
        }

        /// Anyone may pay. The bucket must be TARI and exactly the invoice amount; it is forwarded
        /// to the payee account. Returns a receipt NFT (id = invoice id).
        pub fn pay(&mut self, invoice_id: u64, payment: Bucket) -> Bucket {
            let now = Consensus::current_epoch();
            let inv = self.invoices.get_mut(&invoice_id).expect("unknown invoice");
            assert!(matches!(inv.status, InvoiceStatus::Open), "invoice already paid");
            assert!(now <= inv.expiry_epoch, "invoice expired");
            assert!(payment.resource_address() == TARI_TOKEN, "payment must be TARI");
            assert!(payment.amount() == inv.amount, "payment amount must equal invoice amount exactly");
            inv.status = InvoiceStatus::Paid;
            inv.paid_epoch = now;
            let amount = inv.amount;
            let memo_hash = inv.memo_hash.clone();

            ComponentManager::get(self.payee).invoke("deposit", args![payment]);

            let receipt = ResourceManager::get(self.receipt_resource).mint_non_fungible(
                NonFungibleId::from_u64(invoice_id),
                &metadata![
                    "invoice_id" => invoice_id.to_string(),
                    "amount" => amount.to_string(),
                    "memo_hash" => memo_hash,
                    "paid_epoch" => now.to_string(),
                ],
                &(),
            );
            emit_event("InvoicePaid", metadata!["invoice_id" => invoice_id.to_string(), "amount" => amount.to_string()]);
            receipt
        }

        /// Returns (amount, memo_hash, expiry_epoch, status) where status is
        /// 0 = open, 1 = paid, 2 = expired-unpaid.
        pub fn get_invoice(&self, invoice_id: u64) -> (Amount, String, u64, u8) {
            let inv = self.invoices.get(&invoice_id).expect("unknown invoice");
            let status = match inv.status {
                InvoiceStatus::Paid => 1,
                InvoiceStatus::Open if Consensus::current_epoch() > inv.expiry_epoch => 2,
                InvoiceStatus::Open => 0,
            };
            (inv.amount, inv.memo_hash.clone(), inv.expiry_epoch, status)
        }

        /// True iff the invoice is paid with exactly these terms. Pair with checking that the
        /// holder owns NFT `invoice_id` of `receipt_resource()`.
        pub fn verify_receipt(&self, invoice_id: u64, amount: Amount, memo_hash: String) -> bool {
            match self.invoices.get(&invoice_id) {
                Some(inv) => matches!(inv.status, InvoiceStatus::Paid) && inv.amount == amount && inv.memo_hash == memo_hash,
                None => false,
            }
        }

        pub fn receipt_resource(&self) -> ResourceAddress {
            self.receipt_resource
        }

        pub fn payee(&self) -> ComponentAddress {
            self.payee
        }
    }

    pub struct Invoice {
        pub reference: String,
        pub amount: Amount,
        pub memo_hash: String,
        pub expiry_epoch: u64,
        pub status: InvoiceStatus,
        pub paid_epoch: u64,
    }

    pub enum InvoiceStatus {
        Open,
        Paid,
    }
}
