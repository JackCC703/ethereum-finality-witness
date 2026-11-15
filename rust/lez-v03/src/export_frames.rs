use std::{fs,path::PathBuf};
use finality_witness_lez_v03::{plan,apply,Effect,instance};
use lee_core::{account::{AccountId,ProgramShardSelector,ShardData},program::{PlanInput,AccountMeta,ApplyInput,ApplyOutput,CallKind,GuestOutput},to_borsh_frame};
use serde_json::{Value,json};
fn main(){
 let a:Vec<_>=std::env::args().collect();let seq:Value=serde_json::from_slice(&fs::read(&a[1]).unwrap()).unwrap();let out=PathBuf::from(&a[2]);fs::create_dir_all(&out).unwrap();
 let program=AccountId::new([0x33;32]);let target=instance(program);let mut state=vec![];
 let mut inputs=vec![("initialize".to_string(),json!({"op":"initialize","bootstrap":seq["bootstrap"]}))];
 for (i,s) in seq["steps"].as_array().unwrap().iter().enumerate(){inputs.push((format!("update_{i}"),json!({"op":"update","witness":s["witness"]})));}
 let mut records=vec![];
 for (label,m) in inputs{
  let instruction=borsh::to_vec(&serde_json::to_vec(&m).unwrap()).unwrap();let input=PlanInput{self_account_id:program,caller_account_id:None,accounts:vec![AccountMeta::new(target,false,program)],instruction_data:instruction};
  let output=plan(&input,serde_json::to_vec(&m).unwrap());let effect:Effect=borsh::from_slice(&output.output().effects[0].data).unwrap();
  let apply_input=ApplyInput{self_account_id:program,selector:ProgramShardSelector::new(target,program),pre_data:ShardData::try_from(state.clone()).unwrap(),effect_data:borsh::to_vec(&effect).unwrap()};
  state=apply(effect,&state).unwrap();
  let mut frame=to_borsh_frame(&CallKind::Plan);frame.extend(to_borsh_frame(&input));
  fs::write(out.join(format!("{label}-plan.bin")),frame).unwrap();
  fs::write(out.join(format!("{label}-plan.expected")),to_borsh_frame(&GuestOutput::Plan(output.output().clone()))).unwrap();
  let mut frame=to_borsh_frame(&CallKind::Apply);frame.extend(to_borsh_frame(&apply_input));
  fs::write(out.join(format!("{label}-apply.bin")),frame).unwrap();
  fs::write(out.join(format!("{label}-apply.expected")),to_borsh_frame(&GuestOutput::Apply(ApplyOutput::new(apply_input,Some(ShardData::try_from(state.clone()).unwrap()))))).unwrap();
  records.push(json!({"label":label,"state":serde_json::from_slice::<Value>(&state).unwrap()}));
 }
 fs::write(out.join("states.json"),serde_json::to_vec_pretty(&records).unwrap()).unwrap();println!("Exported {} native plan/apply pairs",records.len());
}
