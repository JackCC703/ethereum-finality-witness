use anyhow::{ensure, Context, Result};
use risc0_zkvm::{default_executor, ExecutorEnv};
use std::{env, fs, time::Instant};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    ensure!(args.len() == 4, "usage: runner GUEST_ELF INPUT_JSON REPORT_JSON");
    ensure!(env::var("RISC0_DEV_MODE").unwrap_or_default() != "1", "unset RISC0_DEV_MODE; this runner measures execution only");
    let raw_elf = fs::read(&args[1]).context("guest ELF")?;
    let binary = risc0_binfmt::ProgramBinary::new(&raw_elf, risc0_zkos_v1compat::V1COMPAT_ELF);
    let image_id = binary.compute_image_id()?.to_string();
    let elf = binary.encode();
    fs::write(format!("{}.bin", &args[1]), &elf)?;
    let input = fs::read(&args[2]).context("input witness")?;
    let length = u32::try_from(input.len())?.to_le_bytes();
    let execution_env = ExecutorEnv::builder()
        .write_slice(&length)
        .write_slice(&input)
        .segment_limit_po2(20)
        .build()?;
    let started = Instant::now();
    let result = default_executor().execute(execution_env, &elf)?;
    let report = serde_json::json!({
        "mode": "risc0_executor_only_no_proof",
        "risc0_version": "3.0.5",
        "image_id": image_id,
        "guest_elf": args[1],
        "input_file": args[2],
        "user_cycles": result.cycles(),
        "segment_count": result.segments.len(),
        "segment_user_cycles": result.segments.iter().map(|s| s.cycles).collect::<Vec<_>>(),
        "wall_seconds": started.elapsed().as_secs_f64(),
        "journal": String::from_utf8(result.journal.bytes)?,
        "limitations": "Execution measurement only: no receipt, zk proof, LEZ transaction, LEZ gas, or testnet deployment."
    });
    let encoded = serde_json::to_string_pretty(&report)?;
    fs::write(&args[3], &encoded)?;
    println!("Saved report to {}; image {}; cycles {}; segments {}; executor seconds {}", &args[3],
             report["image_id"], report["user_cycles"], report["segment_count"], report["wall_seconds"]);
    Ok(())
}
