//! Independent public-testnet v0.3 driver. Never imports local-genesis test keys.
use anyhow::{Result,Context,ensure};
use std::{fs,path::{Path,PathBuf},collections::HashMap,time::{Duration,Instant}};
use serde_json::{Value,json};
use lee::{AccountId,PublicKey,PrivateKey,ProgramShardSelector,program::Program,privacy_preserving_transaction::circuit::ProgramWithDependencies};
use lee_core::program::{PdaSeed,PROGRAM_LOADER_ACCOUNT_ID,ProgramHeader};
use program_loader_core::{MAX_SEGMENT_DATA_LEN,ProgramSegment};
use wallet::{WalletCore,AccountIdentity,config::WalletConfig,program_facades::program_loader::ProgramLoader};
use sequencer_service_rpc::RpcClient;
use common::HashType;
fn read(p:impl AsRef<Path>)->Result<Value>{Ok(serde_json::from_slice(&fs::read(p)?)?)}
fn save(p:impl AsRef<Path>,v:&Value)->Result<()>{let p=p.as_ref();let tmp=p.with_extension("json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(v)?)?;fs::rename(tmp,p)?;Ok(())}
fn open(run:&Path)->Result<WalletCore>{WalletCore::new_update_chain(run.join("wallet_config.json"),run.join("wallet_storage.json"),None)}
async fn compatible(w:&WalletCore)->Result<()>{let p=w.client_owned().get_program_ids().await?;ensure!(p.get("privacy_preserving_circuit")==Some(&lee::PRIVACY_PRESERVING_CIRCUIT_ID),"Remote circuit differs from this v0.3 wallet");ensure!(!risc0_zkvm::ProverOpts::succinct().dev_mode(),"Public deployment refuses development mode");Ok(())}
async fn record(w:&WalletCore,h:HashType,label:&str)->Result<Value>{
 let (tx,b)=tokio::time::timeout(Duration::from_secs(1200),w.poll_transaction(h)).await??;
 let block=w.get_block(b).await?.context("Included block missing")?;
 let i=block.body.transactions.iter().position(|x|x.hash()==h).context("Transaction missing from block")?;
 let _ = i; // v0.3 BlockBody has no per-transaction revert bitmap; verify post-state separately.
 Ok(json!({"label":label,"tx":hex::encode(h.0),"block":b,"inclusion_verified":true,"reverted":null,"transaction":serde_json::to_value(&tx)?}))
}
async fn prepare(run:&Path)->Result<()>{
 fs::create_dir_all(run)?;
 #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(run,fs::Permissions::from_mode(0o700))?;}
 ensure!(!run.join("wallet_storage.json").exists(),"Wallet already exists; refusing overwrite");
 if !run.join("payer.secret").exists() {
  let key=PrivateKey::new_os_random();fs::write(run.join("payer.secret"),key.value())?;
  #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(run.join("payer.secret"),fs::Permissions::from_mode(0o600))?;}
 }
 let bytes:[u8;32]=fs::read(run.join("payer.secret"))?.try_into().map_err(|_|anyhow::anyhow!("Expected 32-byte payer key"))?;
 let payer=AccountId::from(&PublicKey::new_from_private_key(&PrivateKey::try_new(bytes)?));
 if !run.join("payer-public.json").exists(){save(run.join("payer-public.json"),&json!({"payer":payer.to_string()}))?;}
 let mut cfg=WalletConfig::default();cfg.gas_limit=1_000_000;cfg.seq_poll_timeout=Duration::from_secs(2);cfg.seq_tx_poll_max_blocks=1000;
 save(run.join("wallet_config.json"),&serde_json::to_value(cfg)?)?;
 let (mut w,_)=WalletCore::new_init_storage(run.join("wallet_config.json"),run.join("wallet_storage.json"),None,"")?;
 let bytes:[u8;32]=fs::read(run.join("payer.secret"))?.try_into().map_err(|_|anyhow::anyhow!("Expected 32-byte payer key"))?;
 let key=PrivateKey::try_new(bytes)?;let payer=AccountId::from(&PublicKey::new_from_private_key(&key));
 ensure!(payer.to_string()==read(run.join("payer-public.json"))?["payer"].as_str().unwrap(),"Payer identity mismatch");
 w.storage_mut().key_chain_mut().add_imported_public_account(key);w.store_persistent_data()?;
 #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(run.join("wallet_storage.json"),fs::Permissions::from_mode(0o600))?;}
 compatible(&w).await?;
 println!("{}",json!({"payer":payer.to_string(),"balance":w.get_account_balance(payer).await?,"compatible":true}));Ok(())
}
async fn deploy(run:&Path,binary:&Path,sequence:&Path)->Result<()>{
 let mut w=open(run)?;compatible(&w).await?;
 let payer:AccountId=read(run.join("payer-public.json"))?["payer"].as_str().unwrap().parse()?;
 let bytes=fs::read(binary)?;let program=Program::new(bytes.clone().into())?;
 let decoded=risc0_binfmt::ProgramBinary::decode(&bytes)?;
 ensure!(decoded.kernel_elf==risc0_zkos_v1compat::V1COMPAT_ELF,"Unsupported kernel");
 let chunks:Vec<_>=decoded.user_elf.chunks(MAX_SEGMENT_DATA_LEN).collect();
 let result_path=run.join("deployment.json");
 let mut result=if result_path.exists(){let r=read(&result_path)?;ensure!(r["image_id"]==risc0_zkvm::sha::Digest::from(program.id()).to_string(),"Cannot resume with another binary");r}else{
  let header=w.create_new_account_public(None).0;let segments:Vec<String>=(0..chunks.len()).map(|_|w.create_new_account_public(None).0.to_string()).collect();w.store_persistent_data()?;
  let r=json!({"status":"deploying","network":"public_LEZ_v0.3","rpc":w.client_url().to_string(),"payer":payer.to_string(),"program_account":header.to_string(),"image_id":risc0_zkvm::sha::Digest::from(program.id()).to_string(),"segments":segments,"transactions":[],"development_mode":false});save(&result_path,&r)?;r
 };
 let header:AccountId=result["program_account"].as_str().unwrap().parse()?;
 let segments:Vec<AccountId>=result["segments"].as_array().unwrap().iter().map(|x|x.as_str().unwrap().parse()).collect::<std::result::Result<_,_>>()?;
 ensure!(segments.len()==chunks.len(),"Segment count mismatch");
 for i in (0..chunks.len()).rev(){
  let account=w.get_account_view(ProgramShardSelector::new(segments[i],PROGRAM_LOADER_ACCOUNT_ID)).await?;
  if let Some(existing)=ProgramSegment::from_bytes(account.data.shard(PROGRAM_LOADER_ACCOUNT_ID)){
   ensure!(existing.bytecode==chunks[i] && existing.next_segment==segments.get(i+1).copied(),"Existing segment differs");continue;
  }
  let hash=ProgramLoader(&w).write_segment(segments[i],chunks[i].to_vec(),segments.get(i+1).copied(),Some(payer)).await?;
  println!("FW03_SEGMENT_SUBMITTED index={i} tx={hash}");
  let mut rec=record(&w,hash,&format!("deploy_segment_{i}")).await?;
  let account=w.get_account_view(ProgramShardSelector::new(segments[i],PROGRAM_LOADER_ACCOUNT_ID)).await?;
  let written=ProgramSegment::from_bytes(account.data.shard(PROGRAM_LOADER_ACCOUNT_ID)).context("Segment did not settle")?;
  ensure!(written.bytecode==chunks[i] && written.next_segment==segments.get(i+1).copied(),"Wrong segment post-state");rec["post_state_verified"]=json!(true);result["transactions"].as_array_mut().unwrap().push(rec);save(&result_path,&result)?;
 }
 let acc=w.get_account_view(ProgramShardSelector::new(header,PROGRAM_LOADER_ACCOUNT_ID)).await?;
 if let Some(existing)=ProgramHeader::from_bytes(acc.data.shard(PROGRAM_LOADER_ACCOUNT_ID)){
  ensure!(existing.image_id==program.id() && existing.immutable,"Header differs");
 }else{
  let h=ProgramLoader(&w).create_header(header,segments[0],&segments,true,Some(payer)).await?;
  result["transactions"].as_array_mut().unwrap().push(record(&w,h,"deploy_header").await?);save(&result_path,&result)?;
 }
 let header_state=w.get_account_view(ProgramShardSelector::new(header,PROGRAM_LOADER_ACCOUNT_ID)).await?;
 let actual=ProgramHeader::from_bytes(header_state.data.shard(PROGRAM_LOADER_ACCOUNT_ID)).context("Header did not settle")?;
 ensure!(actual.image_id==program.id() && actual.immutable,"Wrong header post-state");
 let instance=AccountId::for_public_pda(&header,&PdaSeed::new([0x46;32]));result["instance"]=json!(instance.to_string());
 let selector=ProgramShardSelector::new(instance,header);let account=w.get_account_view(selector).await?;
 if account.data.shard(header).is_empty(){
  let seq=read(sequence)?;let message=json!({"op":"initialize","bootstrap":seq["bootstrap"]});
  let hash=w.send_pub_tx_paid_by(vec![AccountIdentity::PublicNoSign(instance).select_program_shard(header)],Program::serialize_instruction(serde_json::to_vec(&message)?)?,header,Some(payer)).await?;
  result["transactions"].as_array_mut().unwrap().push(record(&w,hash,"initialize_historical_checkpoint").await?);
 }
 let account=w.get_account_view(selector).await?;let state:Value=serde_json::from_slice(account.data.shard(header))?;
 ensure!(state["schema"]=="fw-v03-historical-v1" && state["finalized_slot"]==7069376,"Unexpected initial state");
 result["public_state"]=state;result["post_state_verified"]=json!(true);result["status"]=json!("deployed_and_initialized");result["limitations"]=json!("Public v0.3 deployment of a historical Capella verifier. No real update claimed until separate accepted proof evidence. Does not fulfill public v0.4 RFP requirement.");save(&result_path,&result)?;println!("{}",json!({"status":result["status"],"program_account":result["program_account"],"instance":result["instance"],"transactions":result["transactions"].as_array().unwrap().len()}));Ok(())
}
async fn update(run:&Path,binary:&Path,sequence:&Path,index:usize)->Result<()>{
 let w=open(run)?;compatible(&w).await?;let deployment=read(run.join("deployment.json"))?;
 let program_id:AccountId=deployment["program_account"].as_str().unwrap().parse()?;let instance:AccountId=deployment["instance"].as_str().unwrap().parse()?;
 let seq=read(sequence)?;let step=seq["steps"].get(index).context("Missing update")?;
 let before=w.get_account_view(ProgramShardSelector::new(instance,program_id)).await?;
 let state:Value=serde_json::from_slice(before.data.shard(program_id))?;ensure!(state["store_commitment"]==step["verified"]["previous_store_commitment"],"Predecessor mismatch");
 let program=Program::new(fs::read(binary)?.into())?;ensure!(deployment["image_id"]==risc0_zkvm::sha::Digest::from(program.id()).to_string(),"Program mismatch");
 let bundle=ProgramWithDependencies::new(program,program_id,HashMap::new());
 let now=Instant::now();let (hash,_)=w.send_privacy_preserving_tx(vec![AccountIdentity::PublicNoSign(instance).select_program_shard(program_id)],Program::serialize_instruction(serde_json::to_vec(&json!({"op":"update","witness":step["witness"]}))?)?,&bundle).await?;
 let mut rec=record(&w,hash,&format!("real_update_{index}")).await?;
 let after=w.get_account_view(ProgramShardSelector::new(instance,program_id)).await?;let state:Value=serde_json::from_slice(after.data.shard(program_id))?;
 ensure!(state["store_commitment"]==step["verified"]["new_store_commitment"],"Successor mismatch");if let common::transaction::LeeTransaction::PrivacyPreserving(tx)=w.get_transaction(hash).await?.context("Missing private transaction")?.0 {
  let raw=tx.witness_set.proof().clone().into_inner();
  let inner:risc0_zkvm::InnerReceipt=borsh::from_slice(&raw)?;ensure!(!matches!(&inner,risc0_zkvm::InnerReceipt::Fake{..}),"Fake receipt");
  let m=&tx.message;
  let output=lee_core::PrivacyPreservingCircuitOutput{public_actions:m.public_actions.iter().map(|a|lee_core::PublicAction{account_id:a.account_id,is_authorized:tx.witness_set.signatures_and_public_keys().iter().any(|(_,pk)|AccountId::from(pk)==a.account_id),effects:a.effects.clone()}).collect(),private_actions:m.private_actions.clone(),block_validity_window:m.block_validity_window,timestamp_validity_window:m.timestamp_validity_window,program_image_claims:m.program_image_claims.clone()};
  let receipt=risc0_zkvm::Receipt::new(inner,output.to_bytes());receipt.verify(lee::PRIVACY_PRESERVING_CIRCUIT_ID)?;
  fs::write(run.join(format!("update-{index}-receipt.bincode")),bincode::serialize(&receipt)?)?;
  fs::write(run.join(format!("update-{index}-journal.bin")),&receipt.journal.bytes)?;
  rec["proof"]=json!({"kind":"cryptographic","independently_verified":true,"circuit_image_id":risc0_zkvm::sha::Digest::from(lee::PRIVACY_PRESERVING_CIRCUIT_ID).to_string(),"bytes":raw.len()});
 } else {anyhow::bail!("Expected private transaction")}
 rec["state"]=state;rec["post_state_verified"]=json!(true);rec["wall_seconds"]=json!(now.elapsed().as_secs_f64());save(run.join(format!("update-{index}.json")),&rec)?;println!("{}",serde_json::to_string_pretty(&rec)?);Ok(())
}
fn state_test(run:&Path,binary:&Path,sequence:&Path)->Result<()> {
 use lee::{V03State,PublicTransaction,public_transaction::{Message,WitnessSet},ValidatedStateDiff};
 let program=Program::new(fs::read(binary)?.into())?;let program_id=AccountId::new([0x33;32]);
 let instance=AccountId::for_public_pda(&program_id,&PdaSeed::new([0x46;32]));
 let mut state=V03State::new().with_named_programs([(program_id,program)]);
 let seq=read(sequence)?;let mut cases=vec![];
 let tx=|account:AccountId,m:Value|->Result<PublicTransaction>{let message=Message::try_new(program_id,vec![ProgramShardSelector::new(account,program_id)],vec![],serde_json::to_vec(&m)?)?;let witnesses=WitnessSet::for_message(&message,&[]);Ok(PublicTransaction::new(message,witnesses))};
 let initialize=json!({"op":"initialize","bootstrap":seq["bootstrap"]});
 let (diff,gas)=ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx(instance,initialize.clone())?,&state,1,1,1_000_000)?;drop(state.apply_state_diff(diff));cases.push(json!({"case":"initialize","cycles":gas.cycles,"budget":1_000_000}));
 for m in [initialize,json!({"op":"apply","verified":seq["steps"][0]["verified"]})] {
  let before=state.get_account_by_id(instance);
  ensure!(ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx(instance,m)?,&state,2,2,1_000_000).is_err(),"Invalid call accepted");
  ensure!(state.get_account_by_id(instance)==before,"Rejected call mutated state");
 }
 let first=json!({"op":"update","witness":seq["steps"][0]["witness"]});
 ensure!(ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx(instance,first)?,&state,2,2,10_000_000).is_err(),"Expected public-cap rejection");
 for (i,step) in seq["steps"].as_array().unwrap().iter().enumerate(){
  let message=json!({"op":"update","witness":step["witness"]});
  // Deliberately larger in-memory budget to test runtime state transitions.
  // This is NOT public-testnet admission; a real update uses the private prover.
  let (diff,gas)=ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx(instance,message.clone())?,&state,i as u64+2,2,20_000_000)?;
  drop(state.apply_state_diff(diff));
  let account=state.get_account_by_id(instance);let after:Value=serde_json::from_slice(account.data.shard(program_id))?;
  ensure!(after["store_commitment"]==step["verified"]["new_store_commitment"] && after["updates_applied"]==i+1,"State-machine result differs");
  ensure!(ValidatedStateDiff::from_public_transaction_with_cycle_budget(&tx(instance,message)?,&state,i as u64+3,3,20_000_000).is_err(),"Replay accepted");
  ensure!(state.get_account_by_id(instance)==account,"Replay mutated state");
  cases.push(json!({"case":format!("update_{i}"),"cycles":gas.cycles,"in_memory_budget":20_000_000,"state":after,"replay_rejected":true}));
 }
 fs::create_dir_all(run)?;save(run.join("state-machine.json"),&json!({"mode":"actual_v03_state_machine_executor_only_no_proofs","network_transactions":0,"initialization_budget":1_000_000,"public_10M_update_rejected":true,"updates_tested_with_explicit_20M_in_memory_budget":true,"reinitialization_and_forged_apply_rejected":true,"cases":cases}))?;
 println!("PASS: actual v0.3 state machine, init, seven successor commitments, replay/forged Apply refusals and public 10M cap rejection");Ok(())
}
#[tokio::main]
async fn main()->Result<()>{let a:Vec<_>=std::env::args().collect();let run=PathBuf::from(a.get(2).context("driver prepare|deploy|update RUN_DIR [BINARY SEQUENCE INDEX]")?);match a[1].as_str(){"self-test"=>state_test(&run,Path::new(&a[3]),Path::new(&a[4])),"check"=>{let w=open(&run)?;compatible(&w).await?;let payer:AccountId=read(run.join("payer-public.json"))?["payer"].as_str().unwrap().parse()?;println!("{}",json!({"compatible":true,"payer":payer.to_string(),"balance":w.get_account_balance(payer).await?,"circuit_image_id":risc0_zkvm::sha::Digest::from(lee::PRIVACY_PRESERVING_CIRCUIT_ID).to_string()}));Ok(())},"prepare"=>prepare(&run).await,"deploy"=>deploy(&run,Path::new(&a[3]),Path::new(&a[4])).await,"update"=>update(&run,Path::new(&a[3]),Path::new(&a[4]),a[5].parse()?).await,_=>anyhow::bail!("Unknown command")}}
