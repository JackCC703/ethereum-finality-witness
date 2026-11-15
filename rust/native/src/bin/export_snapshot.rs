fn main() {
    let file = std::env::args()
        .nth(1)
        .expect("usage: export_snapshot input.json");
    let full: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let store = finality_witness_core::incremental::historical_snapshot(&full).unwrap();
    eprintln!(
        "commitment={}",
        finality_witness_core::incremental::commitment(&store).unwrap()
    );
    println!("{}", serde_json::to_string_pretty(&store).unwrap());
}
