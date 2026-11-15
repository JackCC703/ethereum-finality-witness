//! Atomic native consumer example. Tickets are demonstration counters, never money.
//! No API accepts an externally supplied "verified" JSON report.
use crate::{verify, Root, R};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ConsumerState {
    tickets: u64,
    spent: BTreeSet<String>,
}
impl ConsumerState {
    pub fn snapshot(&self) -> Value {
        json!({"tickets":self.tickets,"spent_statement_ids":self.spent})
    }
    pub fn tickets(&self) -> u64 {
        self.tickets
    }
}

#[derive(Clone, Debug)]
pub struct Policy {
    pub chain_id: u64,
    pub address: [u8; 20],
    pub slot: Root,
    pub minimum: Root,
}
impl Default for Policy {
    fn default() -> Self {
        let address = hex::decode("00000000219ab540356cbb839cbe05303d7705fa")
            .unwrap()
            .try_into()
            .unwrap();
        let mut minimum = [0; 32];
        minimum[31] = 1;
        Self {
            chain_id: 1,
            address,
            slot: [0; 32],
            minimum,
        }
    }
}
fn fixed<const N: usize>(v: &Value) -> R<[u8; N]> {
    hex::decode(
        v.as_str()
            .ok_or("CONSUMER_ENCODING")?
            .trim_start_matches("0x"),
    )
    .map_err(|_| "CONSUMER_ENCODING")?
    .try_into()
    .map_err(|_| "CONSUMER_LENGTH".into())
}

/// Re-run consensus and inclusion verification, enforce policy and uniqueness,
/// then commit exactly one new state. All fallible work precedes mutation.
pub fn consume(state: &mut ConsumerState, input: &Value, policy: &Policy) -> R<Value> {
    let verified = verify(input)?;
    consume_verified(state, &verified, policy)
}
// Private: external callers cannot bypass `verify` by supplying a report.
fn consume_verified(state: &mut ConsumerState, verified: &Value, policy: &Policy) -> R<Value> {
    let chain = verified["chain_id"].as_u64().ok_or("CONSUMER_CHAIN")?;
    if chain != policy.chain_id {
        return Err("CONSUMER_CHAIN".into());
    }
    let fact = &verified["state_witness"];
    let address = fixed::<20>(&fact["address"])?;
    if address != policy.address {
        return Err("CONSUMER_ADDRESS".into());
    }
    let slots = fact["storage"].as_array().ok_or("CONSUMER_STORAGE")?;
    let selected = slots
        .iter()
        .find(|p| fixed::<32>(&p["slot"]).ok() == Some(policy.slot))
        .ok_or("CONSUMER_SLOT")?;
    let value = fixed::<32>(&selected["value"])?;
    if value < policy.minimum {
        return Err("CONSUMER_PREDICATE".into());
    }
    let block_hash = fixed::<32>(&verified["execution_block_hash"])?;
    let mut digest = Sha256::new();
    digest.update(b"finality-witness/consumer/fact/v1\0");
    digest.update(chain.to_be_bytes());
    digest.update(block_hash);
    digest.update(address);
    digest.update(policy.slot);
    // No predicate, RPC metadata, instance, or caller-supplied ID in this identity.
    let id = hex::encode(digest.finalize());
    if state.spent.contains(&id) {
        return Err("CONSUMER_REPLAY".into());
    }
    let tickets = state.tickets.checked_add(1).ok_or("CONSUMER_OVERFLOW")?;
    let mut next = state.clone();
    next.tickets = tickets;
    next.spent.insert(id.clone());
    let result = json!({"operation":"issue_one_demo_ticket","statement_id":id,"chain_id":chain,
        "execution_block":verified["execution_block"],"execution_block_hash":verified["execution_block_hash"],
        "address":hex::encode(address),"slot":hex::encode(policy.slot),"value":hex::encode(value),"tickets":tickets});
    *state = next;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Value {
        serde_json::from_str(include_str!("../../input.json")).unwrap()
    }
    #[test]
    fn valid_then_replay() {
        let mut s = ConsumerState::default();
        let p = Policy::default();
        consume(&mut s, &input(), &p).unwrap();
        assert_eq!(s.tickets(), 1);
        let before = s.clone();
        assert_eq!(
            consume(&mut s, &input(), &p).unwrap_err(),
            "CONSUMER_REPLAY"
        );
        assert_eq!(s, before);
    }
    #[test]
    fn wrong_proof_then_valid_retry() {
        let mut s = ConsumerState::default();
        let before = s.clone();
        let mut x = input();
        x["state_proof"]["result"]["storageProof"][0]["value"] = json!("0x1");
        assert!(consume(&mut s, &x, &Policy::default()).is_err());
        assert_eq!(s, before);
        consume(&mut s, &input(), &Policy::default()).unwrap();
        assert_eq!(s.tickets(), 1);
    }
    #[test]
    fn wrong_chain_unchanged() {
        let mut s = ConsumerState::default();
        let before = s.clone();
        let mut p = Policy::default();
        p.chain_id = 11155111;
        assert_eq!(consume(&mut s, &input(), &p).unwrap_err(), "CONSUMER_CHAIN");
        assert_eq!(s, before);
    }
    #[test]
    fn wrong_address_unchanged() {
        let mut s = ConsumerState::default();
        let before = s.clone();
        let mut p = Policy::default();
        p.address = [0x11; 20];
        assert_eq!(
            consume(&mut s, &input(), &p).unwrap_err(),
            "CONSUMER_ADDRESS"
        );
        assert_eq!(s, before);
    }
    #[test]
    fn wrong_slot_unchanged() {
        let mut s = ConsumerState::default();
        let before = s.clone();
        let mut p = Policy::default();
        p.slot[31] = 1;
        assert_eq!(consume(&mut s, &input(), &p).unwrap_err(), "CONSUMER_SLOT");
        assert_eq!(s, before);
    }
    #[test]
    fn false_predicate_unchanged() {
        let mut s = ConsumerState::default();
        let before = s.clone();
        let mut p = Policy::default();
        p.minimum = [0xff; 32];
        assert_eq!(
            consume(&mut s, &input(), &p).unwrap_err(),
            "CONSUMER_PREDICATE"
        );
        assert_eq!(s, before);
    }
    #[test]
    fn changed_predicate_cannot_bypass_replay() {
        let mut s = ConsumerState::default();
        consume(&mut s, &input(), &Policy::default()).unwrap();
        let before = s.clone();
        let mut p = Policy::default();
        p.minimum = [0; 32];
        assert_eq!(
            consume(&mut s, &input(), &p).unwrap_err(),
            "CONSUMER_REPLAY"
        );
        assert_eq!(s, before);
    }
    #[test]
    fn overflow_unchanged() {
        let mut s = ConsumerState {
            tickets: u64::MAX,
            spent: BTreeSet::new(),
        };
        let before = s.clone();
        assert_eq!(
            consume(&mut s, &input(), &Policy::default()).unwrap_err(),
            "CONSUMER_OVERFLOW"
        );
        assert_eq!(s, before);
    }
}
