use finality_witness_core::consumer::{consume, ConsumerState, Policy};
use serde_json::{json, Value};
fn consumer_demo(input: &Value) -> Value {
    let policy = Policy::default();
    let mut state = ConsumerState::default();
    let mut bad = input.clone();
    bad["state_proof"]["result"]["storageProof"][0]["value"] = json!("0x1");
    let before = state.snapshot();
    let bad_error = consume(&mut state, &bad, &policy).unwrap_err();
    assert_eq!(state.snapshot(), before);
    let accepted = consume(&mut state, input, &policy).unwrap();
    assert_eq!(state.tickets(), 1);
    let before = state.snapshot();
    let replay_error = consume(&mut state, input, &policy).unwrap_err();
    assert_eq!(state.snapshot(), before);
    let mut false_policy = policy.clone();
    false_policy.minimum = [0xff; 32];
    let policy_error = consume(&mut state, input, &false_policy).unwrap_err();
    assert_eq!(state.snapshot(), before);
    json!({"scope":"native_atomic_ticket_consumer_not_LEZ_or_funds","bad_proof":{"error":bad_error,"state_unchanged":true},"valid_retry":accepted,"replay":{"error":replay_error,"state_unchanged":true},"false_predicate":{"error":policy_error,"state_unchanged":true},"final_state":state.snapshot()})
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let is_consumer = args.iter().any(|a| a == "--consumer-demo");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .expect("usage: native [--consumer-demo] input.json");
    let input: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    if is_consumer {
        println!(
            "{}",
            serde_json::to_string_pretty(&consumer_demo(&input)).unwrap()
        );
        return;
    }
    match finality_witness_core::verify(&input) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            eprintln!("REJECTED: {e}");
            std::process::exit(1);
        }
    }
}
