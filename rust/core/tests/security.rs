use finality_witness_core::verify;
use serde_json::{json, Value};
fn input() -> Value {
    serde_json::from_str(include_str!("../../input.json")).unwrap()
}
fn refuses(change: impl FnOnce(&mut Value)) {
    let mut x = input();
    change(&mut x);
    assert!(verify(&x).is_err());
}
#[test]
fn authentic_chain_and_state() {
    let v = verify(&input()).unwrap();
    assert_eq!(v["execution_block"], 17923026);
    assert_eq!(v["chain_id"], 1);
    assert_eq!(v["verified_updates"].as_array().unwrap().len(), 7);
    assert_eq!(
        v["state_witness"]["address"],
        "00000000219ab540356cbb839cbe05303d7705fa"
    );
    assert_eq!(
        v["state_witness"]["storage"][0]["value"],
        "4e585d72ab3a6cb0387fc65e55a7e9bcf6ed555a04e5c1bed4b0ad1264310301"
    );
}
#[test]
fn forged_checkpoint() {
    refuses(|x| {
        x["bootstrap"]["data"]["header"]["beacon"]["state_root"] =
            json!("0x".to_owned() + &"00".repeat(32))
    });
}
#[test]
fn invalid_signature() {
    refuses(|x| {
        x["updates"][0]["data"]["sync_aggregate"]["sync_committee_signature"] =
            json!("0x".to_owned() + &"00".repeat(96))
    });
}
#[test]
fn insufficient_participation() {
    refuses(|x| {
        x["updates"][0]["data"]["sync_aggregate"]["sync_committee_bits"] =
            json!("0x".to_owned() + &"00".repeat(64))
    });
}
#[test]
fn altered_execution_root() {
    refuses(|x| {
        x["finality"]["data"]["finalized_header"]["execution"]["state_root"] =
            json!("0x".to_owned() + &"11".repeat(32))
    });
}
#[test]
fn wrong_account() {
    refuses(|x| x["state_proof"]["result"]["address"] = json!("0x".to_owned() + &"11".repeat(20)));
}
#[test]
fn fabricated_balance() {
    refuses(|x| x["state_proof"]["result"]["balance"] = json!("0x1"));
}
#[test]
fn fabricated_storage_root() {
    refuses(|x| {
        x["state_proof"]["result"]["storageHash"] = json!("0x".to_owned() + &"11".repeat(32))
    });
}
#[test]
fn different_storage_slot() {
    refuses(|x| x["state_proof"]["result"]["storageProof"][0]["key"] = json!("0x1"));
}
#[test]
fn fabricated_storage_value() {
    refuses(|x| x["state_proof"]["result"]["storageProof"][0]["value"] = json!("0x1"));
}
#[test]
fn tampered_storage_proof() {
    refuses(|x| {
        let s = x["state_proof"]["result"]["storageProof"][0]["proof"][0]
            .as_str()
            .unwrap()
            .to_owned();
        let mut bytes = hex::decode(&s[2..]).unwrap();
        let len = bytes.len();
        bytes[len - 1] ^= 1;
        x["state_proof"]["result"]["storageProof"][0]["proof"][0] =
            json!(format!("0x{}", hex::encode(bytes)));
    });
}
#[test]
fn verification_is_read_only() {
    let mut x = input();
    x["state_proof"]["result"]["storageProof"][0]["value"] = json!("0x1");
    let snapshot = x.clone();
    assert!(verify(&x).is_err());
    assert_eq!(x, snapshot);
}
