//! Fixed historical Capella replay. New unaudited integration, not a full light client.
pub mod consumer;
pub mod incremental;
#[cfg(feature = "compact-store")]
pub mod compact;
#[cfg(not(feature = "combined-pairing"))]
use bls12_381::pairing;
use bls12_381::{G1Affine, G1Projective};
#[cfg(not(feature = "blst-backend"))]
use bls12_381::{hash_to_curve::{ExpandMsgXmd, HashToCurve},G2Affine,G2Projective};
use rlp::Rlp;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};
type R<T> = Result<T, String>;
type Root = [u8; 32];
const CP: &str = "5afc212a7924789b2bc86acad3ab3a6ffb1f6e97253ea50bee7f4f51422c9275";
const GENESIS: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
fn need(ok: bool, code: &str) -> R<()> {
    if ok {
        Ok(())
    } else {
        Err(code.into())
    }
}
fn bytes(v: &Value) -> R<Vec<u8>> {
    hex::decode(v.as_str().ok_or("HEX_STRING")?.trim_start_matches("0x"))
        .map_err(|_| "HEX_ENCODING".into())
}
fn rb(v: &Value) -> R<Root> {
    bytes(v)?.try_into().map_err(|_| "ROOT_LENGTH".into())
}
fn num(v: &Value) -> R<u64> {
    let s = v.as_str().ok_or("NUMBER_STRING")?;
    if let Some(x) = s.strip_prefix("0x") {
        u64::from_str_radix(x, 16)
    } else {
        s.parse()
    }
    .map_err(|_| "NUMBER_ENCODING".into())
}
fn h(a: &[u8], b: &[u8]) -> Root {
    let mut x = Sha256::new();
    x.update(a);
    x.update(b);
    x.finalize().into()
}
fn uint(n: u64) -> Root {
    let mut b = [0u8; 32];
    b[..8].copy_from_slice(&n.to_le_bytes());
    b
}
fn merkle(mut xs: Vec<Root>, limit: usize) -> R<Root> {
    need(xs.len() <= limit, "SSZ_LIMIT")?;
    let depth = limit.max(1).next_power_of_two().trailing_zeros();
    let mut z = [0; 32];
    if xs.is_empty() {
        xs.push(z)
    }
    for _ in 0..depth {
        if xs.len() % 2 == 1 {
            xs.push(z)
        }
        xs = xs.chunks_exact(2).map(|p| h(&p[0], &p[1])).collect();
        z = h(&z, &z)
    }
    need(xs.len() == 1, "SSZ_DEPTH")?;
    Ok(xs[0])
}
fn bv(b: &[u8]) -> R<Root> {
    let xs = b
        .chunks(32)
        .map(|c| {
            let mut v = [0; 32];
            v[..c.len()].copy_from_slice(c);
            v
        })
        .collect::<Vec<_>>();
    let n = xs.len().max(1);
    merkle(xs, n)
}
fn bl(b: &[u8], max: usize) -> R<Root> {
    need(b.len() <= max, "BYTE_LIMIT")?;
    let xs = b
        .chunks(32)
        .map(|c| {
            let mut v = [0; 32];
            v[..c.len()].copy_from_slice(c);
            v
        })
        .collect();
    Ok(h(&merkle(xs, (max + 31) / 32)?, &uint(b.len() as u64)))
}
fn beacon(b: &Value) -> R<Root> {
    merkle(
        vec![
            uint(num(&b["slot"])?),
            uint(num(&b["proposer_index"])?),
            rb(&b["parent_root"])?,
            rb(&b["state_root"])?,
            rb(&b["body_root"])?,
        ],
        5,
    )
}
fn exec(e: &Value) -> R<Root> {
    let names = [
        "parent_hash",
        "fee_recipient",
        "state_root",
        "receipts_root",
        "logs_bloom",
        "prev_randao",
        "block_number",
        "gas_limit",
        "gas_used",
        "timestamp",
        "extra_data",
        "base_fee_per_gas",
        "block_hash",
        "transactions_root",
        "withdrawals_root",
    ];
    let mut xs = vec![];
    for k in names {
        xs.push(match k {
            "block_number" | "gas_limit" | "gas_used" | "timestamp" => uint(num(&e[k])?),
            "base_fee_per_gas" => {
                let s = e[k].as_str().ok_or("BASE_FEE")?;
                let mut r = [0; 32];
                for digit in s.bytes() {
                    need(digit.is_ascii_digit(), "BASE_FEE")?;
                    let mut carry = (digit - b'0') as u16;
                    for x in &mut r {
                        let v = *x as u16 * 10 + carry;
                        *x = v as u8;
                        carry = v >> 8;
                    }
                    need(carry == 0, "BASE_FEE_OVERFLOW")?;
                }
                r
            }
            "extra_data" => bl(&bytes(&e[k])?, 32)?,
            _ => bv(&bytes(&e[k])?)?,
        });
    }
    merkle(xs, 15)
}
fn branch(mut leaf: Root, sibs: &Value, mut index: u64, root: Root, code: &str) -> R<()> {
    let a = sibs.as_array().ok_or("BRANCH_ARRAY")?;
    need(a.len() == (63 - index.leading_zeros()) as usize, code)?;
    for b in a {
        let b = rb(b)?;
        leaf = if index & 1 == 1 {
            h(&b, &leaf)
        } else {
            h(&leaf, &b)
        };
        index >>= 1;
    }
    need(index == 1 && leaf == root, code)
}
fn committee(c: &Value) -> R<Root> {
    let keys = c["pubkeys"].as_array().ok_or("KEYS")?;
    need(keys.len() == 512, "COMMITTEE_SIZE")?;
    let ks = keys.iter().map(|p| bv(&bytes(p)?)).collect::<R<Vec<_>>>()?;
    merkle(
        vec![merkle(ks, 512)?, bv(&bytes(&c["aggregate_pubkey"])?)?],
        2,
    )
}
fn header(v: &Value) -> R<()> {
    let s = num(&v["beacon"]["slot"])?;
    need((194048 * 32..269568 * 32).contains(&s), "UNSUPPORTED_FORK")?;
    branch(
        exec(&v["execution"])?,
        &v["execution_branch"],
        25,
        rb(&v["beacon"]["body_root"])?,
        "EXECUTION_BRANCH",
    )
}
fn g1(v: &Value) -> R<G1Affine> {
    let b: [u8; 48] = bytes(v)?.try_into().map_err(|_| "G1_LENGTH")?;
    Option::<G1Affine>::from(G1Affine::from_compressed(&b)).ok_or("INVALID_G1".into())
}
fn signature(c: &Value, agg: &Value, message: Root) -> R<usize> {
    let bits = bytes(&agg["sync_committee_bits"])?;
    need(bits.len() == 64, "BITS_LENGTH")?;
    let count = bits.iter().map(|b| b.count_ones() as usize).sum::<usize>();
    need(count * 3 >= 1024, "INSUFFICIENT_PARTICIPATION")?;
    let mut key = G1Projective::from(g1(&c["aggregate_pubkey"])?);
    for (i, p) in c["pubkeys"].as_array().ok_or("KEYS")?.iter().enumerate() {
        if bits[i / 8] >> (i % 8) & 1 == 0 {
            key -= g1(p)?;
        }
    }
    need(!bool::from(key.is_identity()), "IDENTITY_KEY")?;
    let sig: [u8; 96] = bytes(&agg["sync_committee_signature"])?
        .try_into()
        .map_err(|_| "G2_LENGTH")?;
    #[cfg(feature = "blst-backend")]
    {
        let pk=blst::min_pk::PublicKey::from_bytes(&G1Affine::from(key).to_compressed())
            .map_err(|_| "INVALID_G1")?;
        let sig=blst::min_pk::Signature::from_bytes(&sig).map_err(|_| "INVALID_BLS_SIGNATURE")?;
        need(sig.verify(true,&message,b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_",&[],&pk,false)
            ==blst::BLST_ERROR::BLST_SUCCESS,"INVALID_BLS_SIGNATURE")?;
        return Ok(count);
    }
    #[cfg(not(feature = "blst-backend"))]
    {
    let sig =
        Option::<G2Affine>::from(G2Affine::from_compressed(&sig)).ok_or("INVALID_BLS_SIGNATURE")?;
    need(!bool::from(sig.is_identity()), "IDENTITY_SIG")?;
    let hash = <G2Projective as HashToCurve<ExpandMsgXmd<Sha256>>>::hash_to_curve(
        message,
        b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_",
    );
    #[cfg(not(feature = "combined-pairing"))]
    let valid = pairing(&G1Affine::from(key), &G2Affine::from(hash))
        == pairing(&G1Affine::generator(), &sig);
    #[cfg(feature = "combined-pairing")]
    let valid = {
        use bls12_381::{multi_miller_loop, G2Prepared, Gt};
        let pk = G1Affine::from(key);
        let hashed = G2Prepared::from(G2Affine::from(hash));
        let minus_generator = -G1Affine::generator();
        let prepared_sig = G2Prepared::from(sig);
        multi_miller_loop(&[(&pk, &hashed), (&minus_generator, &prepared_sig)])
            .final_exponentiation()
            == Gt::identity()
    };
    need(valid, "INVALID_BLS_SIGNATURE")?;
    Ok(count)
    }
}
fn keccak(b: &[u8]) -> Root {
    let mut k = Keccak::v256();
    k.update(b);
    let mut r = [0; 32];
    k.finalize(&mut r);
    r
}
fn nibbles(b: &[u8]) -> Vec<u8> {
    b.iter().flat_map(|x| [x >> 4, x & 15]).collect()
}
fn trie(root: Root, key: Root, nodes: &Value) -> R<Vec<u8>> {
    let path = nibbles(&key);
    let mut offset = 0;
    let mut expected = root.to_vec();
    for node in nodes.as_array().ok_or("MPT_NODES")? {
        let data = bytes(node)?;
        need(
            if expected.len() == 32 {
                keccak(&data).as_slice() == expected
            } else {
                data == expected
            },
            "MPT_HASH",
        )?;
        let r = Rlp::new(&data);
        let count = r.item_count().map_err(|_| "MPT_RLP")?;
        if count == 17 {
            if offset == 64 {
                return r.val_at::<Vec<u8>>(16).map_err(|_| "MPT_VALUE".into());
            }
            let child = r.at(path[offset] as usize).map_err(|_| "MPT_CHILD")?;
            expected = if child.is_list() {
                child.as_raw().to_vec()
            } else {
                child.data().map_err(|_| "MPT_CHILD")?.to_vec()
            };
            offset += 1;
        } else if count == 2 {
            let compact = r.val_at::<Vec<u8>>(0).map_err(|_| "MPT_PATH")?;
            need(!compact.is_empty(), "MPT_PATH")?;
            let ns = nibbles(&compact);
            need(ns[0] <= 3, "MPT_FLAGS")?;
            let odd = ns[0] & 1 == 1;
            need(odd || ns[1] == 0, "MPT_PADDING")?;
            let suffix = &ns[if odd { 1 } else { 2 }..];
            need(
                offset + suffix.len() <= 64 && path[offset..offset + suffix.len()] == *suffix,
                "MPT_KEY",
            )?;
            offset += suffix.len();
            if ns[0] >= 2 {
                need(offset == 64, "MPT_KEY_LENGTH")?;
                return r.val_at::<Vec<u8>>(1).map_err(|_| "MPT_VALUE".into());
            }
            let child = r.at(1).map_err(|_| "MPT_CHILD")?;
            expected = if child.is_list() {
                child.as_raw().to_vec()
            } else {
                child.data().map_err(|_| "MPT_CHILD")?.to_vec()
            };
        } else {
            return Err("MPT_NODE_ARITY".into());
        }
    }
    Err("MPT_INCOMPLETE".into())
}
fn word(b: &[u8]) -> R<Root> {
    need(b.len() <= 32, "WORD_LENGTH")?;
    let mut r = [0; 32];
    r[32 - b.len()..].copy_from_slice(b);
    Ok(r)
}
fn hexword(v: &Value) -> R<Root> {
    let s = v.as_str().ok_or("HEX_WORD")?.trim_start_matches("0x");
    let s = if s.len() % 2 == 1 {
        format!("0{s}")
    } else {
        s.to_string()
    };
    word(&hex::decode(s).map_err(|_| "HEX_WORD")?)
}
fn state(root: Root, response: &Value) -> R<Value> {
    let p = &response["result"];
    let address = bytes(&p["address"])?;
    need(address.len() == 20, "ADDRESS_LENGTH")?;
    let account = trie(root, keccak(&address), &p["accountProof"])?;
    let r = Rlp::new(&account);
    need(
        r.item_count().map_err(|_| "ACCOUNT_RLP")? == 4,
        "ACCOUNT_ARITY",
    )?;
    let fields = (0..4)
        .map(|i| r.val_at::<Vec<u8>>(i).map_err(|_| "ACCOUNT_RLP".into()))
        .collect::<R<Vec<_>>>()?;
    need(
        word(&fields[0])? == hexword(&p["nonce"])? && word(&fields[1])? == hexword(&p["balance"])?,
        "ACCOUNT_VALUE",
    )?;
    let storage_root: Root = fields[2]
        .clone()
        .try_into()
        .map_err(|_| "STORAGE_ROOT_LENGTH")?;
    need(
        storage_root == rb(&p["storageHash"])? && fields[3] == bytes(&p["codeHash"])?,
        "ACCOUNT_METADATA",
    )?;
    let witnesses = p["storageProof"].as_array().ok_or("STORAGE_PROOFS")?;
    need(!witnesses.is_empty(), "STORAGE_MISSING")?;
    let mut slots = vec![];
    for s in witnesses {
        let slot = hexword(&s["key"])?;
        let encoded = trie(storage_root, keccak(&slot), &s["proof"])?;
        let value = Rlp::new(&encoded).data().map_err(|_| "STORAGE_RLP")?;
        let value = word(value)?;
        need(value == hexword(&s["value"])?, "STORAGE_VALUE")?;
        slots.push(json!({"slot":hex::encode(slot),"value":hex::encode(value)}));
    }
    Ok(
        json!({"address":hex::encode(address),"balance_be":hex::encode(word(&fields[1])?),"state_root":hex::encode(root),"storage_root":hex::encode(storage_root),"storage":slots}),
    )
}

pub fn verify(input: &Value) -> R<Value> {
    verify_observed(input, &mut |_| {})
}

/// Observe phase boundaries without replacing or bypassing any verification.
pub fn verify_observed(input: &Value, observe: &mut impl FnMut(&str)) -> R<Value> {
    let b = &input["bootstrap"]["data"];
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
    let mut fin = b["header"].clone();
    let mut current = b["current_sync_committee"].clone();
    let mut next: Option<Value> = None;
    let mut updates = input["updates"].as_array().ok_or("UPDATES")?.clone();
    updates.push(input["finality"].clone());
    let mut verified = vec![];
    observe("bootstrap_complete");
    for (update_index, wrapped) in updates.into_iter().enumerate() {
        let u = &wrapped["data"];
        let a = &u["attested_header"];
        let f = &u["finalized_header"];
        let ss = num(&u["signature_slot"])?;
        let at = num(&a["beacon"]["slot"])?;
        let fs = num(&f["beacon"]["slot"])?;
        let old = num(&fin["beacon"]["slot"])?;
        let period = old / 8192;
        need(7109432 >= ss && ss > at && at >= fs, "SLOT_ORDER")?;
        need(
            fs > old || next.is_none() && !u["next_sync_committee"].is_null(),
            "STALE_UPDATE",
        )?;
        need(
            ss / 8192 == period || ss / 8192 == period + 1,
            "SIGNATURE_PERIOD",
        )?;
        let c = if ss / 8192 == period {
            &current
        } else {
            next.as_ref().ok_or("UNKNOWN_COMMITTEE")?
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
                committee(&u["next_sync_committee"])?,
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
        observe(&format!("update_{update_index}_before_bls"));
        let signers = signature(c, &u["sync_aggregate"], h(&beacon(&a["beacon"])?, &domain))?;
        observe(&format!("update_{update_index}_after_bls"));
        if fs / 8192 == period + 1 {
            current = next.take().ok_or("MISSING_HANDOFF")?;
            if !u["next_sync_committee"].is_null() {
                next = Some(u["next_sync_committee"].clone());
            }
        } else {
            need(fs / 8192 == period, "FINALITY_PERIOD")?;
            if next.is_none() && !u["next_sync_committee"].is_null() {
                next = Some(u["next_sync_committee"].clone());
            }
        }
        if fs > old {
            fin = f.clone();
        }
        verified.push(json!({"slot":fs,"signers":signers}));
        observe(&format!("update_{update_index}_complete"));
    }
    let witness = state(rb(&fin["execution"]["state_root"])?, &input["state_proof"])?;
    observe("account_and_storage_complete");
    Ok(
        json!({"checkpoint":CP,"chain_id":1,"execution_block_hash":fin["execution"]["block_hash"],"finalized_slot":num(&fin["beacon"]["slot"])? ,"execution_block":num(&fin["execution"]["block_number"])? ,"verified_updates":verified,"state_witness":witness,"scope":"historical_capella_native_or_risc0_execution_not_LEZ"}),
    )
}
