//! Deterministic witnesses, derived by verifying every predecessor from CP.
use finality_witness_core::incremental::{advance_store, bootstrap_store, commitment, verify_update};
use serde_json::{json, Value};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "export_sequence INPUT_JSON OUTPUT_JSON");
    let full: Value = serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let mut store = bootstrap_store(&full).unwrap();
    let initial = store.clone();
    let mut steps = Vec::new();
    for (index, update) in full["updates"].as_array().unwrap().iter()
        .chain(std::iter::once(&full["finality"])).enumerate() {
        let mut witness = json!({"store":store,"update":update});
        if index == 6 { witness["state_proof"] = full["state_proof"].clone(); }
        let verified = verify_update(&witness).unwrap();
        store = advance_store(&store, update).unwrap().0;
        assert_eq!(verified["new_store_commitment"], commitment(&store).unwrap());
        steps.push(json!({"index":index,"witness":witness,"verified":verified}));
    }
    let sequence = json!({"schema":"fw-capella-sequence-v1","bootstrap":full["bootstrap"],
        "initial_store":initial,"steps":steps,"final_store":store,
        "limitations":"Historical Capella replay, not live Ethereum following."});
    std::fs::write(&args[2], serde_json::to_vec_pretty(&sequence).unwrap()).unwrap();
    println!("Exported seven sequential, independently verified transitions to {}", args[2]);
}
