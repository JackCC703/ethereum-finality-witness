use super::*;
use lee_core::account::ActorState;
fn full() -> Value { serde_json::from_str(include_str!("../../input.json")).unwrap() }
fn input(from: Option<Actor>, state: &[u8]) -> ReceiveInput {
    ReceiveInput { receiver: Actor::new(AccountId::new([2;32]),AccountId::new([3;32])),
        from, is_authorized:false,pre_state:ActorState::from(state.to_vec()),message:vec![] }
}
fn own() -> Option<Actor> { Some(Actor::new(AccountId::new([5;32]),AccountId::new([3;32]))) }
fn msg(v: Value)->Vec<u8>{ serde_json::to_vec(&v).unwrap() }
fn init()->Vec<u8>{handle(&input(None,&[]),msg(json!({"op":"initialize","bootstrap":full()["bootstrap"]}))).post_state.unwrap().to_vec()}
fn verify(w:Value)->Vec<u8>{
    let response=handle(&input(None,&[]),msg(json!({"op":"verify","public_instance":AccountId::new([2;32]).to_string(),"witness":w})));
    assert_eq!(response.sync_messages.len(),1);
    borsh::from_slice(&response.sync_messages[0].message).unwrap()
}
fn first()->Value { let f=full();json!({"store":bootstrap_store(&f).unwrap(),"update":f["updates"][0]}) }
fn refuses(i:ReceiveInput,m:Vec<u8>){ let before=i.pre_state.clone();assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||handle(&i,m))).is_err());assert_eq!(before,i.pre_state); }
#[test]
fn seven_sequential_updates_and_committee_rotations(){
    let f=full();let mut store=bootstrap_store(&f).unwrap();let mut state=init();let initial=decode(&state);let mut rotations=0;
    for (index,u) in f["updates"].as_array().unwrap().iter().chain(std::iter::once(&f["finality"])).enumerate(){
        let mut w=json!({"store":store,"update":u});if index==6 { w["state_proof"]=f["state_proof"].clone(); }
        let m=verify(w);let next=advance_store_for_test(&store,u);if next["current_sync_committee"]!=store["current_sync_committee"]{rotations+=1;}
        state=handle(&input(own(),&state),m).post_state.unwrap().to_vec();store=next;
        let public=decode(&state);assert_eq!(public["updates_applied"],index+1);assert_eq!(public["store_commitment"],commitment(&store).unwrap());assert!(state.len()<1024);
        if index==0{assert_eq!(public["finalized_slot"],initial["finalized_slot"]);assert_ne!(public["store_commitment"],initial["store_commitment"]);}
    }
    assert!(rotations>=4);assert_eq!(decode(&state)["finalized_slot"],7109344);assert_eq!(decode(&state)["execution_block"],17923026);
}
fn advance_store_for_test(s:&Value,u:&Value)->Value {finality_witness_core::incremental::advance_store(s,u).unwrap().0}
#[test]fn rejects_external_apply(){refuses(input(None,&init()),verify(first()));}
#[test]fn rejects_foreign_program_apply(){refuses(input(Some(Actor::new(AccountId::new([5;32]),AccountId::new([9;32]))),&init()),verify(first()));}
#[test]fn rejects_replay(){let m=verify(first());let next=handle(&input(own(),&init()),m.clone()).post_state.unwrap();refuses(input(own(),&next),m);}
#[test]fn rejects_witness_not_bound_to_public_state(){let w:Value=serde_json::from_str(include_str!("../../incremental-input.json")).unwrap();refuses(input(own(),&init()),verify(w));}
#[test]fn rejects_bad_bootstrap(){let mut b=full()["bootstrap"].clone();b["data"]["header"]["beacon"]["slot"]=json!("7069377");refuses(input(None,&[]),msg(json!({"op":"initialize","bootstrap":b})));}
#[test]fn rejects_repeat_initialize(){refuses(input(None,&init()),msg(json!({"op":"initialize","bootstrap":full()["bootstrap"]})));}
#[test]fn rejects_bad_signature(){let mut w=first();w["update"]["data"]["sync_aggregate"]["sync_committee_signature"]=json!("0x".to_owned()+&"00".repeat(96));refuses(input(None,&[]),msg(json!({"op":"verify","public_instance":AccountId::new([2;32]).to_string(),"witness":w})));}
#[test]fn rejects_valid_curve_signature_from_another_update(){let mut w=first();w["update"]["data"]["sync_aggregate"]["sync_committee_signature"]=full()["updates"][1]["data"]["sync_aggregate"]["sync_committee_signature"].clone();refuses(input(None,&[]),msg(json!({"op":"verify","public_instance":AccountId::new([2;32]).to_string(),"witness":w})));}
#[test]fn rejects_below_two_thirds_participation(){let mut w=first();w["update"]["data"]["sync_aggregate"]["sync_committee_bits"]=json!("0x".to_owned()+&"01".repeat(64));refuses(input(None,&[]),msg(json!({"op":"verify","public_instance":AccountId::new([2;32]).to_string(),"witness":w})));}
