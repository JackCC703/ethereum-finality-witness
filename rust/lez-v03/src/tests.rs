use super::*;
use lee_core::program::AccountMeta;
fn input() -> PlanInput {
 let program=AccountId::new([0x33;32]);
 PlanInput{self_account_id:program,caller_account_id:None,accounts:vec![AccountMeta::new(instance(program),false,program)],instruction_data:vec![]}
}
fn sequence()->Value{serde_json::from_str(include_str!("../../compact-sequence.json")).unwrap()}
fn planned(input:&PlanInput,m:Value)->Effect {
 let plan=plan(input,serde_json::to_vec(&m).unwrap());
 borsh::from_slice(&plan.output().effects[0].data).unwrap()
}
fn initial()->Vec<u8>{apply(planned(&input(),json!({"op":"initialize","bootstrap":sequence()["bootstrap"]})),&[]).unwrap()}
#[test] fn bootstrap_is_authenticated(){let s:Value=serde_json::from_slice(&initial()).unwrap();assert_eq!(s["finalized_slot"],7069376);assert_eq!(s["updates_applied"],0);}
#[test] fn seven_updates_match_reference(){let seq=sequence();let mut state=initial();for (i,step) in seq["steps"].as_array().unwrap().iter().enumerate(){state=apply(planned(&input(),json!({"op":"update","witness":step["witness"]})),&state).unwrap();let after:Value=serde_json::from_slice(&state).unwrap();assert_eq!(after["store_commitment"],step["verified"]["new_store_commitment"]);assert_eq!(after["updates_applied"],i+1);}}
#[test] #[should_panic(expected="FW03_ALREADY_INITIALIZED")] fn cannot_reinitialize(){let s=initial();apply(planned(&input(),json!({"op":"initialize","bootstrap":sequence()["bootstrap"]})),&s);}
#[test] #[should_panic(expected="FW03_INSTANCE")] fn rejects_other_instance(){let mut i=input();i.accounts[0].account_id=AccountId::new([0x12;32]);planned(&i,json!({"op":"initialize","bootstrap":sequence()["bootstrap"]}));}
#[test] #[should_panic(expected="FW03_SHARD")] fn rejects_other_program_shard(){let mut i=input();i.accounts[0].program_account_id=AccountId::new([0x12;32]);planned(&i,json!({"op":"initialize","bootstrap":sequence()["bootstrap"]}));}
#[test] #[should_panic(expected="FW03_UNKNOWN_OPERATION")] fn caller_cannot_inject_apply(){planned(&input(),json!({"op":"apply","verified":sequence()["steps"][0]["verified"]}));}
#[test] #[should_panic(expected="FW03_STALE_STORE")] fn rejects_replay(){let step=&sequence()["steps"][0];let effect=planned(&input(),json!({"op":"update","witness":step["witness"]}));let state=apply(effect.clone(),&initial()).unwrap();apply(effect,&state);}
#[test] #[should_panic(expected="FW03_BOOTSTRAP")] fn rejects_corrupt_bootstrap(){let mut s=sequence();s["bootstrap"]["data"]["header"]["beacon"]["slot"]=json!("1");planned(&input(),json!({"op":"initialize","bootstrap":s["bootstrap"]}));}
#[test] #[should_panic(expected="FW03_SLOT_ORDER")] fn rejects_finality_regression(){let mut v=sequence()["steps"][0]["verified"].clone();v["finalized_slot"]=json!(1);apply(Effect::Advance(serde_json::to_vec(&v).unwrap()),&initial());}
#[test] #[should_panic(expected="FW03_CHAIN")] fn rejects_wrong_chain_effect(){let mut v=sequence()["steps"][0]["verified"].clone();v["chain_id"]=json!(2);apply(Effect::Advance(serde_json::to_vec(&v).unwrap()),&initial());}
