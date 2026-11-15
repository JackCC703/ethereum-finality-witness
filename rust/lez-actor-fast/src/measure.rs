//! Execution measurement of an actual actor binary with official Borsh framing.
use std::{fs,time::Instant};
use risc0_zkvm::{default_executor,ExecutorEnv};
fn main(){
    let a:Vec<_>=std::env::args().collect();assert_eq!(a.len(),4,"measure-actor PROGRAM_BIN FRAMED_INPUT REPORT_JSON");
    let binary=fs::read(&a[1]).unwrap();let frame=fs::read(&a[2]).unwrap();
    let env=ExecutorEnv::builder().write_slice(&frame).segment_limit_po2(18).build().unwrap();
    let started=Instant::now();let session=default_executor().execute(env,&binary).unwrap();
    assert_eq!(session.exit_code,risc0_zkvm::ExitCode::Halted(0),"actor execution failed");
    let report=serde_json::json!({"mode":"actor_execution_only_no_proof","image_id":risc0_binfmt::compute_image_id(&binary).unwrap().to_string(),
        "user_cycles":session.cycles(),"segments":session.segments.len(),"segment_po2":18,
        "wall_seconds":started.elapsed().as_secs_f64(),"journal_bytes":session.journal.bytes.len()});
    fs::write(format!("{}.journal.bin",a[3]),session.journal.bytes).unwrap();
    fs::write(&a[3],serde_json::to_vec_pretty(&report).unwrap()).unwrap();println!("{report}");
}
