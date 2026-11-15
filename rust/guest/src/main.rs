#![no_main]
risc0_zkvm::guest::entry!(main);
fn main() {
    let mut length = [0u8; 4];
    risc0_zkvm::guest::env::read_slice(&mut length);
    let size = u32::from_le_bytes(length) as usize;
    assert!(size <= 4_000_000, "INPUT_LIMIT");
    let mut bytes = vec![0; size];
    risc0_zkvm::guest::env::read_slice(&mut bytes);
    let input: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    let mut previous = risc0_zkvm::guest::env::cycle_count();
    let decoded_at = previous;
    let mut measurements = vec![];
    let result = finality_witness_core::verify_observed(&input, &mut |phase| {
        let now = risc0_zkvm::guest::env::cycle_count();
        measurements.push(serde_json::json!({"phase":phase,"cycle_count":now,"delta_cycles":now-previous}));
        previous = now;
    });
    let output = match result {
        Ok(result) => serde_json::json!({"accepted":true,"result":result,"input_decoded_at_cycle":decoded_at,"phase_measurements":measurements}),
        Err(error) => serde_json::json!({"accepted":false,"error":error,"phase_measurements":measurements}),
    };
    risc0_zkvm::guest::env::commit_slice(&serde_json::to_vec(&output).unwrap());
}
