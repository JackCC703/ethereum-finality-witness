//! Stateful historical Capella verification. A consumer must authenticate the
//! previous store commitment against its own public state before applying a result.
//! Bootstrap is anchored to CP; imported stores are not independently trusted.
use crate::{
    beacon, branch, committee, h, header, need, num, rb, signature, state, Root, CP, GENESIS, R,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub const PINNED_SNAPSHOT_SHA256: &str =
    "76084610a9e4dfe1aea101c87bf9734b3d7a5a423d0d63f222c2b75ecfdc3aac";

pub fn commitment(store: &Value) -> R<String> {
    let encoded = serde_json::to_vec(store).map_err(|_| "STORE_ENCODING")?;
    let mut hash = Sha256::new();
    hash.update(b"finality-witness/verified-store/v1\0");
    hash.update(encoded);
    Ok(hex::encode(hash.finalize()))
}
pub fn bootstrap_store(input: &Value) -> R<Value> {
    let b = &input["bootstrap"]["data"];
    #[cfg(feature = "compact-store")]
    if !b["current_sync_committee"]["pubkeys_root"].is_null() {
        return crate::compact::bootstrap(input);
    }
    header(&b["header"])?;
    need(
        hex::encode(beacon(&b["header"]["beacon"])?) == CP,
        "CHECKPOINT_MISMATCH",
    )?;
    branch(
        committee(&b["current_sync_committee"])?,
        &b["current_sync_committee_branch"],
        54,
        rb(&b["header"]["beacon"]["state_root"])?,
        "BOOTSTRAP_COMMITTEE",
    )?;
    Ok(
        json!({"schema":"fw-historical-capella-store-v1","chain_id":1,"checkpoint":CP,
        "finalized_header":b["header"],"current_sync_committee":b["current_sync_committee"],"next_sync_committee":null}),
    )
}
pub fn advance_store(store: &Value, wrapped: &Value) -> R<(Value, usize)> {
    let compact = compact_profile(store);
    need(
        store["chain_id"] == 1
            && store["checkpoint"] == CP
            && (store["schema"] == "fw-historical-capella-store-v1" || compact),
        "STORE_PROFILE",
    )?;
    let u = &wrapped["data"];
    let a = &u["attested_header"];
    let f = &u["finalized_header"];
    let ss = num(&u["signature_slot"])?;
    let at = num(&a["beacon"]["slot"])?;
    let fs = num(&f["beacon"]["slot"])?;
    let old = num(&store["finalized_header"]["beacon"]["slot"])?;
    let period = old / 8192;
    let next = &store["next_sync_committee"];
    need((194048 * 32..269568 * 32).contains(&ss), "UNSUPPORTED_SIGNATURE_FORK")?;
    need(ss > at && at >= fs, "SLOT_ORDER")?;
    need(
        fs > old || next.is_null() && !u["next_sync_committee"].is_null(),
        "STALE_UPDATE",
    )?;
    need(
        ss / 8192 == period || ss / 8192 == period + 1,
        "SIGNATURE_PERIOD",
    )?;
    let c = if ss / 8192 == period {
        &store["current_sync_committee"]
    } else {
        need(!next.is_null(), "UNKNOWN_COMMITTEE")?;
        next
    };
    header(a)?;
    header(f)?;
    branch(
        beacon(&f["beacon"])?,
        &u["finality_branch"],
        105,
        rb(&a["beacon"]["state_root"])?,
        "FINALITY_BRANCH",
    )?;
    if !u["next_sync_committee"].is_null() {
        branch(
            committee_root(&u["next_sync_committee"], compact)?,
            &u["next_sync_committee_branch"],
            55,
            rb(&a["beacon"]["state_root"])?,
            "NEXT_COMMITTEE",
        )?;
    }
    let mut version = [0; 32];
    version[..4].copy_from_slice(&[3, 0, 0, 0]);
    let genesis: Root = hex::decode(GENESIS).unwrap().try_into().unwrap();
    let fr = h(&version, &genesis);
    let mut domain = [0; 32];
    domain[..4].copy_from_slice(&[7, 0, 0, 0]);
    domain[4..].copy_from_slice(&fr[..28]);
    #[cfg(feature = "compact-store")]
    let reconstructed = if compact { Some(crate::compact::signing_committee(c, &u["sync_aggregate"], &u["non_signer_witnesses"])?) } else { None };
    #[cfg(feature = "compact-store")]
    let signing_committee = reconstructed.as_ref().unwrap_or(c);
    #[cfg(not(feature = "compact-store"))]
    let signing_committee = c;
    let signers = signature(signing_committee, &u["sync_aggregate"], h(&beacon(&a["beacon"])?, &domain))?;
    let mut updated = store.clone();
    if fs / 8192 == period + 1 {
        need(!next.is_null(), "MISSING_HANDOFF")?;
        updated["current_sync_committee"] = next.clone();
        updated["next_sync_committee"] = stored_committee(&u["next_sync_committee"], compact)?;
    } else {
        need(fs / 8192 == period, "FINALITY_PERIOD")?;
        if next.is_null() && !u["next_sync_committee"].is_null() {
            updated["next_sync_committee"] = stored_committee(&u["next_sync_committee"], compact)?;
        }
    }
    if fs > old {
        updated["finalized_header"] = stored_header(f, compact)?;
    }
    Ok((updated, signers))
}

fn compact_profile(store: &Value) -> bool {
    #[cfg(feature = "compact-store")]
    { store["schema"] == crate::compact::SCHEMA }
    #[cfg(not(feature = "compact-store"))]
    { let _ = store; false }
}
fn committee_root(c: &Value, compact: bool) -> R<Root> {
    #[cfg(feature = "compact-store")]
    if compact { return crate::compact::committee_root(c); }
    let _ = compact;
    committee(c)
}
fn stored_header(header: &Value, compact: bool) -> R<Value> {
    #[cfg(feature = "compact-store")]
    if compact { return crate::compact::stored_header(header); }
    let _ = compact;
    Ok(header.clone())
}
fn stored_committee(c: &Value, compact: bool) -> R<Value> {
    #[cfg(feature = "compact-store")]
    if compact && !c.is_null() { return crate::compact::canonical_descriptor(c); }
    let _ = compact;
    Ok(c.clone())
}

/// Deterministically rebuild the demo seed from the original trusted checkpoint.
/// This does real native consensus verification; it is not itself a zk proof.
pub fn historical_snapshot(input: &Value) -> R<Value> {
    let mut store = bootstrap_store(input)?;
    let updates = input["updates"].as_array().ok_or("UPDATES")?;
    need(updates.len() == 6, "SNAPSHOT_HISTORY_LENGTH")?;
    for update in updates {
        store = advance_store(&store, update)?.0;
    }
    Ok(store)
}

/// Compatibility wrapper for the original fixed-snapshot execution artifact.
pub fn verify_pinned_update(input: &Value) -> R<Value> {
    need(commitment(&input["store"])? == PINNED_SNAPSHOT_SHA256, "UNAUTHENTICATED_STORE")?;
    verify_update(input)
}

/// Verify a successor. The caller must bind previous_store_commitment to its
/// authenticated state (the LEZ actor does this in same-program Apply).
pub fn verify_update(input: &Value) -> R<Value> {
    let store = &input["store"];
    let previous = commitment(store)?;
    let (updated, signers) = advance_store(store, &input["update"])?;
    let fin = &updated["finalized_header"];
    let mut result = json!({"chain_id":1,"checkpoint":CP,"previous_store_commitment":previous,
        "new_store_commitment":commitment(&updated)?,"finalized_slot":num(&fin["beacon"]["slot"])? ,
        "finalized_beacon_root":hex::encode(beacon(&fin["beacon"])?) ,
        "execution_block":num(&fin["execution"]["block_number"])? ,
        "execution_block_hash":fin["execution"]["block_hash"],"state_root":fin["execution"]["state_root"],
        "signers":signers,"trust_boundary":"previous store commitment must match authenticated consumer state; historical Capella only"});
    if !input["state_proof"].is_null() {
        result["state_witness"] =
            state(rb(&fin["execution"]["state_root"])?, &input["state_proof"])?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn full() -> Value {
        serde_json::from_str(include_str!("../../input.json")).unwrap()
    }
    fn input() -> Value {
        let f = full();
        json!({"store":historical_snapshot(&f).unwrap(),"update":f["finality"],"state_proof":f["state_proof"]})
    }
    #[test]
    fn snapshot_is_reproducibly_pinned() {
        assert_eq!(
            commitment(&historical_snapshot(&full()).unwrap()).unwrap(),
            PINNED_SNAPSHOT_SHA256
        );
    }
    #[test]
    fn final_update_matches_full_replay() {
        let i = input();
        let r = verify_pinned_update(&i).unwrap();
        let f = crate::verify(&full()).unwrap();
        assert_eq!(r["execution_block"], f["execution_block"]);
        assert_eq!(r["state_witness"], f["state_witness"]);
    }
    #[test]
    fn rejects_fake_snapshot_root() {
        let mut i = input();
        i["store"]["finalized_header"]["execution"]["state_root"] =
            json!("0x".to_owned() + &"11".repeat(32));
        assert_eq!(
            verify_pinned_update(&i).unwrap_err(),
            "UNAUTHENTICATED_STORE"
        );
    }
    #[test]
    fn rejects_tampered_signature_without_mutation() {
        let mut i = input();
        i["update"]["data"]["sync_aggregate"]["sync_committee_signature"] =
            json!("0x".to_owned() + &"00".repeat(96));
        let before = i.clone();
        assert!(verify_pinned_update(&i).is_err());
        assert_eq!(i, before);
    }
    #[test]
    fn rejects_replayed_old_update() {
        let mut i = input();
        i["update"] = full()["updates"][5].clone();
        assert_eq!(verify_pinned_update(&i).unwrap_err(), "STALE_UPDATE");
    }
}
