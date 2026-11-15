//! Separate LEZ v0.3 plan/apply adapter. Does not modify the v0.4 actor.
use borsh::{BorshDeserialize, BorshSerialize};
use finality_witness_core::incremental::{bootstrap_store, commitment, verify_update};
use lee_core::{account::AccountId, program::{PdaSeed, PlanInput, Plan}};
use serde_json::{Value, json};

pub const INSTANCE_SEED: PdaSeed = PdaSeed::new([0x46; 32]);
pub fn instance(program: AccountId) -> AccountId { AccountId::for_public_pda(&program, &INSTANCE_SEED) }

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum Effect { Initialize(Vec<u8>), Advance(Vec<u8>) }

/// Runtime verifies the plan's input echo and binds effects to this program.
/// Caller instruction bytes can never directly select an Apply effect.
pub fn plan(input: &PlanInput, instruction: Vec<u8>) -> Plan {
    assert_eq!(input.accounts.len(), 1, "FW03_ACCOUNTS");
    let target = &input.accounts[0];
    assert_eq!(target.account_id, instance(input.self_account_id), "FW03_INSTANCE");
    assert_eq!(target.program_account_id, input.self_account_id, "FW03_SHARD");
    let m: Value = serde_json::from_slice(&instruction).expect("FW03_JSON");
    let effect = match m["op"].as_str().expect("FW03_OPERATION") {
        "initialize" => {
            let store = bootstrap_store(&json!({"bootstrap":m["bootstrap"]})).expect("FW03_BOOTSTRAP");
            let header = &store["finalized_header"];
            let state = json!({"schema":"fw-v03-historical-v1", "chain_id":1,
                "store_commitment":commitment(&store).unwrap(),
                "finalized_slot":header["beacon"]["slot"].as_str().unwrap().parse::<u64>().unwrap(),
                "execution_block":header["execution"]["block_number"].as_str().unwrap().parse::<u64>().unwrap(),
                "updates_applied":0});
            Effect::Initialize(serde_json::to_vec(&state).unwrap())
        },
        "update" => {
            let result=verify_update(&m["witness"]).expect("FW03_VERIFY");
            assert_eq!(result["chain_id"],1,"FW03_CHAIN");
            Effect::Advance(serde_json::to_vec(&result).unwrap())
        },
        _ => panic!("FW03_UNKNOWN_OPERATION"),
    };
    let mut plan=Plan::new(input);
    plan.effect(target,&effect);
    plan
}

/// Only effects emitted by a verified Plan reach this runtime entrypoint.
/// Current state is checked here, rather than trusting the predecessor supplied to Plan.
pub fn apply(effect: Effect, previous: &[u8]) -> Option<Vec<u8>> {
    match effect {
        Effect::Initialize(state) => {
            assert!(previous.is_empty(),"FW03_ALREADY_INITIALIZED");
            Some(state)
        },
        Effect::Advance(bytes) => {
            let old:Value=serde_json::from_slice(previous).expect("FW03_NOT_INITIALIZED");
            let verified:Value=serde_json::from_slice(&bytes).expect("FW03_EFFECT");
            assert_eq!(old["schema"],"fw-v03-historical-v1","FW03_SCHEMA");
            assert_eq!(old["chain_id"],1,"FW03_CHAIN");
            assert_eq!(verified["chain_id"],old["chain_id"],"FW03_CHAIN");
            assert_eq!(verified["previous_store_commitment"],old["store_commitment"],"FW03_STALE_STORE");
            assert_ne!(verified["new_store_commitment"],old["store_commitment"],"FW03_UNCHANGED_STORE");
            assert_eq!(verified["new_store_commitment"].as_str().unwrap().len(),64,"FW03_COMMITMENT");
            assert!(verified["finalized_slot"].as_u64().unwrap()>=old["finalized_slot"].as_u64().unwrap(),"FW03_SLOT_ORDER");
            let count=old["updates_applied"].as_u64().unwrap().checked_add(1).expect("FW03_OVERFLOW");
            Some(serde_json::to_vec(&json!({"schema":"fw-v03-historical-v1","chain_id":1,
                "store_commitment":verified["new_store_commitment"],"finalized_slot":verified["finalized_slot"],
                "execution_block":verified["execution_block"],"execution_block_hash":verified["execution_block_hash"],
                "finalized_beacon_root":verified["finalized_beacon_root"],"state_root":verified["state_root"],
                "updates_applied":count})).unwrap())
        }
    }
}
#[cfg(test)]
mod tests;
