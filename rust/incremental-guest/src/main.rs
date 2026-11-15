#![no_main]
risc0_zkvm::guest::entry!(main);
fn main(){
    let mut size=[0u8;4];risc0_zkvm::guest::env::read_slice(&mut size);let size=u32::from_le_bytes(size) as usize;assert!(size<=4_000_000,"INPUT_LIMIT");
    let mut bytes=vec![0;size];risc0_zkvm::guest::env::read_slice(&mut bytes);let input:serde_json::Value=serde_json::from_slice(&bytes).expect("JSON");
    let start=risc0_zkvm::guest::env::cycle_count();
    let result=finality_witness_core::incremental::verify_pinned_update(&input);
    let end=risc0_zkvm::guest::env::cycle_count();
    let out=match result{Ok(v)=>serde_json::json!({"accepted":true,"result":v,"verification_cycles":end-start}),Err(e)=>serde_json::json!({"accepted":false,"error":e,"verification_cycles":end-start})};
    risc0_zkvm::guest::env::commit_slice(&serde_json::to_vec(&out).unwrap());
}
