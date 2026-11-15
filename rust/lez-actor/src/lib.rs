//! Stateful historical Capella Verify -> Apply LEZ v0.4 demonstration.
//! The public instance stores only authenticated summary/commitment, never keys.
use finality_witness_core::incremental::{bootstrap_store, commitment, verify_update};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};
use serde_json::{Value, json};
use std::str::FromStr;
fn decode(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("FW_JSON")
}

#[cfg(test)]
mod tests;
fn number(value: &Value) -> u64 {
    value.as_u64().expect("FW_NUMBER")
}

/// Messages are Borsh Vec<u8> containing JSON. `apply` is internal-only.
pub fn handle(input: &ReceiveInput, message: Vec<u8>) -> Response {
    let m = decode(&message);
    match m["op"].as_str().expect("FW_OPERATION") {
        "initialize" => {
            assert!(input.pre_state.is_empty(), "FW_ALREADY_INITIALIZED");
            // Independently authenticate the bootstrap against the fixed checkpoint.
            let store = bootstrap_store(&json!({"bootstrap":m["bootstrap"]}))
                .unwrap_or_else(|e| panic!("FW_BOOTSTRAP:{e}"));
            let header = &store["finalized_header"];
            let initial = json!({"schema":"fw-public-instance-v2","chain_id":1,
                "store_commitment":commitment(&store).unwrap(),
                "finalized_slot":header["beacon"]["slot"].as_str().unwrap().parse::<u64>().unwrap(),
                "execution_block":header["execution"]["block_number"].as_str().unwrap().parse::<u64>().unwrap(),
                "updates_applied":0});
            Response::set_state(serde_json::to_vec(&initial).unwrap())
        }
        "verify" => {
            let target = AccountId::from_str(m["public_instance"].as_str().expect("FW_INSTANCE"))
                .expect("FW_INSTANCE");
            let result =
                verify_update(&m["witness"]).unwrap_or_else(|e| panic!("FW_VERIFY:{e}"));
            assert_eq!(result["chain_id"], 1, "FW_CHAIN");
            let apply = json!({"op":"apply","verified":result});
            Response::keep_state().send_sync(
                Actor::new(target, input.receiver.program_account_id),
                &serde_json::to_vec(&apply).unwrap(),
            )
        }
        "apply" => {
            assert!(input.from_own_program(), "FW_APPLY_AUTH");
            assert!(!input.pre_state.is_empty(), "FW_NOT_INITIALIZED");
            let old = decode(&input.pre_state);
            assert_eq!(old["schema"], "fw-public-instance-v2", "FW_INSTANCE_SCHEMA");
            let v = &m["verified"];
            assert_eq!(v["chain_id"], old["chain_id"], "FW_CHAIN");
            assert_eq!(
                v["previous_store_commitment"], old["store_commitment"],
                "FW_STALE_STORE"
            );
            assert!(
                number(&v["finalized_slot"]) >= number(&old["finalized_slot"]),
                "FW_SLOT_ORDER"
            );
            assert_eq!(
                v["new_store_commitment"]
                    .as_str()
                    .expect("FW_COMMITMENT")
                    .len(),
                64,
                "FW_COMMITMENT"
            );
            assert_ne!(v["new_store_commitment"], old["store_commitment"], "FW_UNCHANGED_STORE");
            let count = number(&old["updates_applied"])
                .checked_add(1)
                .expect("FW_OVERFLOW");
            let updated = json!({"schema":"fw-public-instance-v2","chain_id":1,
                "store_commitment":v["new_store_commitment"],"finalized_slot":v["finalized_slot"],
                "execution_block":v["execution_block"],"execution_block_hash":v["execution_block_hash"],
                "finalized_beacon_root":v["finalized_beacon_root"],"state_root":v["state_root"],
                "updates_applied":count});
            Response::set_state(serde_json::to_vec(&updated).unwrap())
        }
        _ => panic!("FW_UNKNOWN_OPERATION"),
    }
}

