//! Executes the compiled OotleInvoice WASM template in the official Tari Ootle engine
//! harness (`tari_template_test_tooling`). In-process only: no network, wallet or funds.
use tari_template_test_tooling::{
    TemplateTest,
    crypto::RistrettoSecretKey,
    engine_types::virtual_substate::{VirtualSubstate, VirtualSubstateId},
    template_lib_types::{Amount, ComponentAddress, NonFungibleAddress, ResourceAddress, constants::TARI_TOKEN},
    transaction::args,
};

const MEMO: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const PRICE: u64 = 2_500_000; // 2.5 TARI in micro-units

struct Ctx {
    test: TemplateTest,
    book: ComponentAddress,
    payee: ComponentAddress,
    payee_proof: NonFungibleAddress,
}

fn setup() -> Ctx {
    let mut test = TemplateTest::new(env!("CARGO_MANIFEST_DIR"), ["."]);
    let (payee, payee_proof, _sk) = test.create_empty_account();
    let owner = test.owner_proof();
    let book: ComponentAddress = test.call_function("OotleInvoice", "new", args![payee], vec![owner]);
    Ctx { test, book, payee, payee_proof }
}

fn set_epoch(test: &mut TemplateTest, epoch: u64) {
    test.set_virtual_substate(VirtualSubstateId::CurrentEpoch, VirtualSubstate::CurrentEpoch(epoch));
}

fn create(c: &mut Ctx, reference: &str, amount: u64, expiry: u64) -> u64 {
    let owner = c.test.owner_proof();
    c.test.call_method(
        c.book,
        "create_invoice",
        args![reference.to_string(), Amount::from(amount), MEMO.to_string(), expiry],
        vec![owner],
    )
}

fn balance(c: &mut Ctx, account: ComponentAddress, proof: NonFungibleAddress, res: ResourceAddress) -> Amount {
    c.test.call_method(account, "balance", args![res], vec![proof])
}

/// Payer withdraws `amount` TARI from their account and calls pay; receipt goes back to payer.
fn pay_tx(
    c: &Ctx,
    payer: ComponentAddress,
    sk: &RistrettoSecretKey,
    id: u64,
    amount: u64,
) -> tari_template_test_tooling::transaction::Transaction {
    c.test
        .transaction()
        .call_method(payer, "withdraw", args![TARI_TOKEN, Amount::from(amount)])
        .put_last_instruction_output_on_workspace("payment")
        .call_method(c.book, "pay", args![id, Workspace("payment")])
        .put_last_instruction_output_on_workspace("receipt")
        .call_method(payer, "deposit", args![Workspace("receipt")])
        .build_and_seal(sk)
}

#[test]
fn exact_payment_forwards_funds_and_issues_receipt() {
    let mut c = setup();
    let id = create(&mut c, "INV-001", PRICE, 100);
    let (payer, payer_proof, sk) = c.test.create_funded_account();
    let tx = pay_tx(&c, payer, &sk, id, PRICE);
    c.test.execute_expect_success(tx, vec![payer_proof.clone()]);

    let (payee, pp) = (c.payee, c.payee_proof.clone());
    assert_eq!(balance(&mut c, payee, pp, TARI_TOKEN), Amount::from(PRICE), "payee received exact amount");
    let receipt_res: ResourceAddress = c.test.call_method(c.book, "receipt_resource", args![], vec![]);
    assert_eq!(balance(&mut c, payer, payer_proof, receipt_res), Amount::from(1u64), "payer holds receipt NFT");

    let inv: (Amount, String, u64, u8) = c.test.call_method(c.book, "get_invoice", args![id], vec![]);
    assert_eq!(inv.3, 1, "status paid");
    let ok: bool = c.test.call_method(c.book, "verify_receipt", args![id, Amount::from(PRICE), MEMO.to_string()], vec![]);
    assert!(ok);
    let bad: bool = c.test.call_method(c.book, "verify_receipt", args![id, Amount::from(PRICE + 1), MEMO.to_string()], vec![]);
    assert!(!bad, "receipt must not verify with wrong amount");
}

#[test]
fn wrong_amount_is_rejected_atomically() {
    let mut c = setup();
    let id = create(&mut c, "INV-002", PRICE, 100);
    let (payer, payer_proof, sk) = c.test.create_funded_account();
    let before = balance(&mut c, payer, payer_proof.clone(), TARI_TOKEN);
    for wrong in [PRICE - 1, PRICE + 1] {
        let tx = pay_tx(&c, payer, &sk, id, wrong);
        let r = c.test.execute_expect_failure(tx, vec![payer_proof.clone()]);
        assert!(format!("{r:?}").contains("exactly"), "unexpected reason: {r:?}");
    }
    assert_eq!(balance(&mut c, payer, payer_proof, TARI_TOKEN), before, "payer funds untouched");
    let inv: (Amount, String, u64, u8) = c.test.call_method(c.book, "get_invoice", args![id], vec![]);
    assert_eq!(inv.3, 0, "still open");
}

#[test]
fn duplicate_payment_is_rejected() {
    let mut c = setup();
    let id = create(&mut c, "INV-003", PRICE, 100);
    let (payer, payer_proof, sk) = c.test.create_funded_account();
    c.test.execute_expect_success(pay_tx(&c, payer, &sk, id, PRICE), vec![payer_proof.clone()]);
    let r = c.test.execute_expect_failure(pay_tx(&c, payer, &sk, id, PRICE), vec![payer_proof]);
    assert!(format!("{r:?}").contains("already paid"), "unexpected reason: {r:?}");
    let (payee, pp) = (c.payee, c.payee_proof.clone());
    assert_eq!(balance(&mut c, payee, pp, TARI_TOKEN), Amount::from(PRICE), "payee paid once only");
}

#[test]
fn create_invoice_is_idempotent_per_reference() {
    let mut c = setup();
    let a = create(&mut c, "ORDER-42", PRICE, 100);
    let b = create(&mut c, "ORDER-42", PRICE, 100);
    assert_eq!(a, b);
    let other = create(&mut c, "ORDER-43", PRICE, 100);
    assert_ne!(a, other);
    // same reference, different terms -> rejected
    let owner = c.test.owner_proof();
    let tx = c
        .test
        .transaction()
        .call_method(c.book, "create_invoice", args!["ORDER-42".to_string(), Amount::from(PRICE + 1), MEMO.to_string(), 100u64])
        .build_and_seal(c.test.secret_key());
    let r = c.test.execute_expect_failure(tx, vec![owner]);
    assert!(format!("{r:?}").contains("different terms"), "unexpected reason: {r:?}");
}

#[test]
fn expired_invoice_cannot_be_paid() {
    let mut c = setup();
    let id = create(&mut c, "INV-004", PRICE, 5);
    let (payer, payer_proof, sk) = c.test.create_funded_account();
    set_epoch(&mut c.test, 6);
    let inv: (Amount, String, u64, u8) = c.test.call_method(c.book, "get_invoice", args![id], vec![]);
    assert_eq!(inv.3, 2, "reported as expired");
    let r = c.test.execute_expect_failure(pay_tx(&c, payer, &sk, id, PRICE), vec![payer_proof]);
    assert!(format!("{r:?}").contains("expired"), "unexpected reason: {r:?}");
}

#[test]
fn payment_on_expiry_epoch_is_accepted() {
    let mut c = setup();
    let id = create(&mut c, "INV-005", PRICE, 5);
    let (payer, payer_proof, sk) = c.test.create_funded_account();
    set_epoch(&mut c.test, 5);
    c.test.execute_expect_success(pay_tx(&c, payer, &sk, id, PRICE), vec![payer_proof]);
}

#[test]
fn invalid_invoice_inputs_are_rejected() {
    let mut c = setup();
    let owner = c.test.owner_proof();
    let cases: Vec<(String, u64, String, u64, &str)> = vec![
        ("ok".into(), 0, MEMO.into(), 100, "amount must be positive"),
        ("ok".into(), 1, "zz".into(), 100, "memo_hash"),
        ("bad ref<script>".into(), 1, MEMO.into(), 100, "reference must be"),
        ("ok".into(), 1, MEMO.into(), 0, "expiry must be in the future"),
    ];
    for (r, amt, memo, exp, why) in cases {
        let tx = c
            .test
            .transaction()
            .call_method(c.book, "create_invoice", args![r, Amount::from(amt), memo, exp])
            .build_and_seal(c.test.secret_key());
        let res = c.test.execute_expect_failure(tx, vec![owner.clone()]);
        assert!(format!("{res:?}").contains(why), "expected '{why}', got {res:?}");
    }
}

#[test]
fn only_owner_can_create_invoices() {
    let mut c = setup();
    let (_acc, stranger_proof, sk) = c.test.create_funded_account();
    let tx = c
        .test
        .transaction()
        .call_method(c.book, "create_invoice", args!["X".to_string(), Amount::from(1u64), MEMO.to_string(), 100u64])
        .build_and_seal(&sk);
    c.test.execute_expect_failure(tx, vec![stranger_proof]);
}
