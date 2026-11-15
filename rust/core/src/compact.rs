//! Authenticated committee roots with Merkle witnesses for exactly the non-signers.
//! Aggregates remain bound to the Ethereum SyncCommittee root. No host validity flag.
use crate::{Root,R,CP,beacon,branch,bv,bytes,h,header,merkle,need,num,rb};
use crate::incremental::{advance_store,bootstrap_store,verify_update};
use serde_json::{json,Value};
pub const SCHEMA:&str="fw-compact-capella-store-v1";
fn members(c:&Value)->R<Vec<Root>> {
    let keys=c["pubkeys"].as_array().ok_or("KEYS")?;need(keys.len()==512,"COMMITTEE_SIZE")?;
    keys.iter().map(|p| {let b=bytes(p)?;need(b.len()==48,"G1_LENGTH")?;bv(&b)}).collect()
}
pub fn descriptor(c:&Value)->R<Value>{
    let root=merkle(members(c)?,512)?;
    let aggregate=bytes(&c["aggregate_pubkey"])?;need(aggregate.len()==48,"G1_LENGTH")?;
    Ok(json!({"pubkeys_root":hex::encode(root),"aggregate_pubkey":hex::encode(aggregate)}))
}
pub fn committee_root(c:&Value)->R<Root>{
    need(c.as_object().ok_or("COMMITTEE_DESCRIPTOR")?.len()==2,"COMMITTEE_DESCRIPTOR")?;
    let aggregate=bytes(&c["aggregate_pubkey"])?;need(aggregate.len()==48,"G1_LENGTH")?;
    Ok(h(&rb(&c["pubkeys_root"])?,&bv(&aggregate)?))
}
pub fn canonical_descriptor(c:&Value)->R<Value>{
    committee_root(c)?;
    Ok(json!({"pubkeys_root":hex::encode(rb(&c["pubkeys_root"])?),"aggregate_pubkey":hex::encode(bytes(&c["aggregate_pubkey"])?) }))
}
pub fn stored_header(v:&Value)->R<Value>{
    let b=&v["beacon"];let e=&v["execution"];
    let root=|x:&Value|->R<String>{Ok(format!("0x{}",hex::encode(rb(x)?)))};
    Ok(json!({"beacon":{"slot":num(&b["slot"])?.to_string(),"proposer_index":num(&b["proposer_index"])?.to_string(),
        "parent_root":root(&b["parent_root"])? ,"state_root":root(&b["state_root"])? ,"body_root":root(&b["body_root"])?},
        "execution":{"block_number":num(&e["block_number"])?.to_string(),"block_hash":root(&e["block_hash"])? ,"state_root":root(&e["state_root"])?}}))
}
pub fn bootstrap(input:&Value)->R<Value>{
    let b=&input["bootstrap"]["data"];header(&b["header"])?;
    need(hex::encode(beacon(&b["header"]["beacon"])?)==CP,"CHECKPOINT_MISMATCH")?;
    branch(committee_root(&b["current_sync_committee"])?,&b["current_sync_committee_branch"],54,
        rb(&b["header"]["beacon"]["state_root"])?,"BOOTSTRAP_COMMITTEE")?;
    Ok(json!({"schema":SCHEMA,"chain_id":1,"checkpoint":CP,"finalized_header":stored_header(&b["header"])? ,
        "current_sync_committee":canonical_descriptor(&b["current_sync_committee"])? ,"next_sync_committee":null}))
}
pub fn signing_committee(c:&Value,aggregate:&Value,hints:&Value)->R<Value>{
    committee_root(c)?;
    let bits=bytes(&aggregate["sync_committee_bits"])?;need(bits.len()==64,"BITS_LENGTH")?;
    let count=bits.iter().map(|b|b.count_ones() as usize).sum::<usize>();need(count*3>=1024,"INSUFFICIENT_PARTICIPATION")?;
    let hints=hints.as_array().ok_or("NON_SIGNER_WITNESSES")?;need(hints.len()==512-count,"NON_SIGNER_COUNT")?;
    let mut keys=vec![Value::Null;512];let mut position=0;
    for i in 0..512 {
        if bits[i/8]>>(i%8)&1==0 {
            let witness=&hints[position];need(witness["index"].as_u64()==Some(i as u64),"NON_SIGNER_INDEX")?;
            let key=bytes(&witness["pubkey"])?;need(key.len()==48,"G1_LENGTH")?;
            branch(bv(&key)?,&witness["branch"],512+i as u64,rb(&c["pubkeys_root"])?,"NON_SIGNER_MEMBERSHIP")?;
            keys[i]=witness["pubkey"].clone();position+=1;
        }
    }
    Ok(json!({"aggregate_pubkey":c["aggregate_pubkey"],"pubkeys":keys}))
}
fn non_signers(c:&Value,aggregate:&Value)->R<Value>{
    let bits=bytes(&aggregate["sync_committee_bits"])?;need(bits.len()==64,"BITS_LENGTH")?;
    let mut levels=vec![members(c)?];
    while levels.last().unwrap().len()>1 {let next=levels.last().unwrap().chunks_exact(2).map(|x|h(&x[0],&x[1])).collect();levels.push(next);}
    let mut hints=vec![];
    for i in 0..512 {if bits[i/8]>>(i%8)&1==0 {let mut index=i;let mut proof=vec![];for level in levels.iter().take(9){proof.push(hex::encode(level[index^1]));index/=2;}
        hints.push(json!({"index":i,"pubkey":c["pubkeys"][i],"branch":proof}));}}
    Ok(json!(hints))
}
/// Derive compact witnesses from a fully verified source sequence; verify again compactly.
pub fn sequence(full:&Value)->R<Value>{
    let mut raw=bootstrap_store(full)?;let mut bootstrap=full["bootstrap"].clone();
    bootstrap["data"]["current_sync_committee"]=descriptor(&bootstrap["data"]["current_sync_committee"])?;
    let mut store=bootstrap_store(&json!({"bootstrap":bootstrap}))?;let initial=store.clone();let mut steps=vec![];
    let updates=full["updates"].as_array().ok_or("UPDATES")?;
    for (index,wrapped) in updates.iter().chain(std::iter::once(&full["finality"])).enumerate(){
        let mut update=wrapped.clone();let u=&mut update["data"];
        let current=num(&raw["finalized_header"]["beacon"]["slot"])?/8192;
        let c=if num(&u["signature_slot"])?/8192==current{&raw["current_sync_committee"]}else{&raw["next_sync_committee"]};
        u["non_signer_witnesses"]=non_signers(c,&u["sync_aggregate"])?;
        if !u["next_sync_committee"].is_null(){u["next_sync_committee"]=descriptor(&u["next_sync_committee"])?;}
        let mut witness=json!({"store":store,"update":update});
        if index==updates.len(){witness["state_proof"]=full["state_proof"].clone();}
        let verified=verify_update(&witness)?;
        store=advance_store(&store,&witness["update"])?.0;raw=advance_store(&raw,wrapped)?.0;
        need(verified["finalized_slot"].as_u64()==Some(num(&raw["finalized_header"]["beacon"]["slot"])?),"COMPACT_PARITY")?;
        steps.push(json!({"index":index,"witness":witness,"verified":verified}));
    }
    Ok(json!({"schema":"fw-compact-capella-sequence-v1","bootstrap":bootstrap,"initial_store":initial,"steps":steps,"final_store":store,
        "limitations":"Historical Capella only; previous commitments require authenticated consumer state."}))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn full()->Value{serde_json::from_str(include_str!("../../input.json")).unwrap()}
    #[test]fn seven_compact_transitions_match_full(){let s=sequence(&full()).unwrap();assert_eq!(s["steps"][6]["verified"]["execution_block"],17923026);assert_eq!(s["steps"][6]["verified"]["state_witness"],crate::verify(&full()).unwrap()["state_witness"]);}
    #[test]fn refuses_tampered_non_signer_key(){let mut s=sequence(&full()).unwrap();let w=&mut s["steps"][0]["witness"];w["update"]["data"]["non_signer_witnesses"][0]["pubkey"]=json!("00".repeat(48));assert_eq!(verify_update(w).unwrap_err(),"NON_SIGNER_MEMBERSHIP");}
    #[test]fn refuses_wrong_non_signer_index(){let mut s=sequence(&full()).unwrap();let w=&mut s["steps"][0]["witness"];w["update"]["data"]["non_signer_witnesses"][0]["index"]=json!(999);assert_eq!(verify_update(w).unwrap_err(),"NON_SIGNER_INDEX");}
    #[test]fn descriptor_matches_full_committee_root(){let f=full();let c=&f["bootstrap"]["data"]["current_sync_committee"];assert_eq!(committee_root(&descriptor(c).unwrap()).unwrap(),crate::committee(c).unwrap());}
}
