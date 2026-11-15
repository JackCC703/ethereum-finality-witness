fn main(){
    let a:Vec<_>=std::env::args().collect();assert_eq!(a.len(),3,"export-compact-sequence INPUT_JSON OUTPUT_JSON");
    let full:serde_json::Value=serde_json::from_slice(&std::fs::read(&a[1]).unwrap()).unwrap();
    let sequence=finality_witness_core::compact::sequence(&full).unwrap();
    std::fs::write(&a[2],serde_json::to_vec_pretty(&sequence).unwrap()).unwrap();
    println!("Exported seven independently verified compact successors");
}
