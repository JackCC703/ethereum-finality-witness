//! Local ephemeral demo wallets and local-genesis test funds only.
use anyhow::{Context, Result, ensure};
use common::HashType;
use lee::{AccountId, Actor, PrivateKey, PublicKey, program::Program,
          privacy_preserving_transaction::circuit::ProgramCatalog};
use sequencer_service_rpc::{RpcClient, SequencerClient};
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}, time::{Duration, Instant}};
use wallet::{AccountIdentity, AsyncDelivery, WalletCore,
             config::WalletConfig,
             program_facades::{CreditDelivery, native_token_transfer::NativeTokenTransfer}};

fn read(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn write(path: impl AsRef<Path>, value: &Value) -> Result<()> {
    Ok(fs::write(path, serde_json::to_vec_pretty(value)?)?)
}
fn open(run: &Path) -> Result<WalletCore<SequencerClient>> {
    WalletCore::new_update_chain(run.join("wallet_config.json"), run.join("wallet_storage.json"), None, "")
}
fn prepare(run: &Path, source: &Path) -> Result<()> {
    fs::create_dir_all(run)?;
    ensure!(!run.join("wallet_storage.json").exists(), "Use a fresh run directory; refusing to replace a wallet");
    let mut config = WalletConfig::default();
    config.sequencer.sequencer_addr = "http://127.0.0.1:3040".parse()?;
    config.gas_limit = 10_000_000;
    config.seq_poll_timeout = Duration::from_secs(1);
    config.seq_tx_poll_max_blocks = 120;
    write(run.join("wallet_config.json"), &serde_json::to_value(config)?)?;
    let (mut wallet, _) = WalletCore::new_init_storage(
        run.join("wallet_config.json"), run.join("wallet_storage.json"), None, "")?;
    // Publicly known test key; never use it with assets outside this isolated genesis.
    let test_key = PrivateKey::try_new([42; 32])?;
    let test_pk = PublicKey::new_from_private_key(&test_key);
    let payer = AccountId::from(&test_pk);
    wallet.storage_mut().key_chain_mut().add_imported_public_account(test_key);
    let instance = wallet.create_new_account_public(None).0;
    // Official, publicly known local-test identity already has a genesis recovery binding.
    let genesis_private = testnet_initial_state::initial_priv_accounts_private_keys().remove(0);
    let verifier = genesis_private.account_id();
    wallet.storage_mut().key_chain_mut().add_imported_private_account(
        genesis_private.key_chain,genesis_private.chain_index,genesis_private.account);
    wallet.store_persistent_data()?;
    let accounts = json!({"payer":payer.to_string(),"instance":instance.to_string(),"verifier":verifier.to_string()});
    write(run.join("accounts.json"), &accounts)?;
    let mut genesis = read(source.join("lez/sequencer/service/networks/local.json"))?;
    genesis["block_create_timeout"] = json!("2s");
    genesis["genesis"]["actions"].as_array_mut().context("genesis actions")?.push(
        json!({"supply_account":{"recipient":{"identified":{"key":test_pk.to_string()}},"balance":1_000_000_000_000_000u64}}));
    // A pending genesis credit is claimed by the first private update; no fake funding receipt is needed.
    genesis["genesis"]["actions"].as_array_mut().unwrap().push(json!({
        "supply_private_account":{"address":verifier.to_string(),"balance":100_000_000_000_000u64}}));
    write(run.join("local-demo.json"), &genesis)?;
    println!("{}", serde_json::to_string_pretty(&accounts)?);
    Ok(())
}

async fn run_sequence(run: &Path, program_path: &Path, sequence_path: &Path, limit: usize) -> Result<()> {
    let ids=read(run.join("accounts.json"))?;
    let payer:AccountId=ids["payer"].as_str().unwrap().parse()?;
    let instance:AccountId=ids["instance"].as_str().unwrap().parse()?;
    let verifier:AccountId=ids["verifier"].as_str().unwrap().parse()?;
    let sequence=read(sequence_path)?;
    let mut wallet=open(run)?;wallet.sync_to_latest_block().await?;
    let bytes=fs::read(program_path)?;
    let program=Program::new(bytes.clone().into())?;
    let program_id=program_deployment::deploy_program(&mut wallet,bytes,payer).await?;
    let catalog:ProgramCatalog=[(program_id,program)].into_iter().collect();
    let actor=Actor::new(instance,program_id);
    let (init,_)=wallet.send_tx(vec![AccountIdentity::Public(payer).balance(),
        AccountIdentity::Public(instance).select_program_actor_state(program_id)],1,
        Program::serialize_message(serde_json::to_vec(&json!({"op":"initialize","bootstrap":sequence["bootstrap"]}))?)?,
        &catalog,None,AsyncDelivery::default()).await?;
    wait_and_sync(&mut wallet,init).await?;
    let init_record=transaction(&wallet,init,"initialize_from_trusted_checkpoint").await?;
    ensure!(init_record["reverted"]==false,"bootstrap initialization reverted");
    let initial=snapshot(&wallet,actor).await?;
    let mut steps=vec![init_record];
    let mut states=vec![initial.clone()];
    wallet.set_private_fee_payer(Some(verifier));
    let count=limit.min(sequence["steps"].as_array().context("sequence steps")?.len());
    for index in 0..count {
        let step=&sequence["steps"][index];
        let before=snapshot(&wallet,actor).await?;
        ensure!(before["store_commitment"]==step["verified"]["previous_store_commitment"],"sequence predecessor mismatch");
        println!("FW_SEQUENCE_BEGIN index={index} previous={}",before["store_commitment"]);
        let started=Instant::now();
        let (hash,_)=wallet.send_privacy_preserving_tx(
            vec![AccountIdentity::PrivateOwned(verifier).select_program_actor_state(program_id),
                 AccountIdentity::PublicNoSign(instance).select_program_actor_state(program_id)],0,
            Program::serialize_message(serde_json::to_vec(&json!({"op":"verify","public_instance":instance.to_string(),"witness":step["witness"]}))?)?,&catalog).await?;
        wait_and_sync(&mut wallet,hash).await?;
        let mut record=transaction(&wallet,hash,&format!("verified_update_{index}")).await?;
        record["wall_seconds"]=json!(started.elapsed().as_secs_f64());
        ensure!(record["reverted"]==false,"valid sequential update reverted");
        let after=snapshot(&wallet,actor).await?;
        ensure!(after["updates_applied"]==index+1,"update count mismatch");
        ensure!(after["store_commitment"]==step["verified"]["new_store_commitment"],"successor commitment mismatch");
        states.push(after.clone());steps.push(record);
        let result=json!({"mode":"local_LEZ_v0.4.0_rc1_standalone","risc0_dev_mode":std::env::var("RISC0_DEV_MODE").ok(),
            "program_account":program_id.to_string(),"instance":instance.to_string(),"updates_applied":index+1,
            "funding":"local-genesis private pending credit claimed in first update",
            "initial":initial,"after":after,"states":states,"steps":steps,
            "limitations":"Historical Capella sequence; standalone mock Bedrock; no public-testnet or live-follower claim."});
        write(run.join("sequence-result.json"),&result)?;
        println!("FW_SEQUENCE_APPLIED index={index} finalized_slot={} block={}",after["finalized_slot"],after["execution_block"]);
    }
    // Replay is checked through the actual wallet's full private/public simulation.
    let before=snapshot(&wallet,actor).await?;
    let replay=wallet.send_privacy_preserving_tx(
        vec![AccountIdentity::PrivateOwned(verifier).select_program_actor_state(program_id),AccountIdentity::PublicNoSign(instance).select_program_actor_state(program_id)],0,
        Program::serialize_message(serde_json::to_vec(&json!({"op":"verify","public_instance":instance.to_string(),"witness":sequence["steps"][count-1]["witness"]}))?)?,&catalog).await;
    let error=match replay {Ok(_)=>anyhow::bail!("replay accepted"),Err(e)=>format!("{e}")};
    ensure!(snapshot(&wallet,actor).await?==before,"replay mutated state");
    let mut result=read(run.join("sequence-result.json"))?;
    result["replay"]=json!({"stage":"wallet_preflight_not_submitted","error":error,"state_unchanged":true});
    write(run.join("sequence-result.json"),&result)?;
    println!("FW_SEQUENCE_COMPLETE updates={count} dev_mode={:?}",std::env::var("RISC0_DEV_MODE").ok());
    Ok(())
}

fn prove(binary: &Path, input_path: &Path, output: &Path) -> Result<()> {
    use risc0_zkvm::{default_prover,ExecutorEnv,ProverOpts,InnerReceipt};
    ensure!(!ProverOpts::succinct().dev_mode(),"real proof refuses development mode");
    fs::create_dir_all(output)?;
    let bytes=fs::read(binary)?;
    let image_id=risc0_binfmt::compute_image_id(&bytes)?;
    let input=fs::read(input_path)?;
    let env=ExecutorEnv::builder().write_slice(&(input.len() as u32).to_le_bytes()).write_slice(&input).segment_limit_po2(18).build()?;
    let started=Instant::now();
    let info=default_prover().prove_with_opts(env,&bytes,&ProverOpts::succinct())?;
    ensure!(!matches!(info.receipt.inner,InnerReceipt::Fake{..}),"fake receipt");
    info.receipt.verify(image_id)?;
    fs::write(output.join("receipt.bincode"),bincode::serialize(&info.receipt)?)?;
    fs::write(output.join("journal.json"),&info.receipt.journal.bytes)?;
    write(output.join("report.json"),&json!({"image_id":image_id.to_string(),"wall_seconds":started.elapsed().as_secs_f64(),
        "receipt_kind":"succinct","receipt_verified":true,"risc0_dev_mode":false,"stats":format!("{:?}",info.stats)}))?;
    Ok(())
}
async fn snapshot(wallet: &WalletCore<SequencerClient>, actor: Actor) -> Result<Value> {
    let account = wallet.get_account_view(actor).await?.context("instance does not exist")?;
    Ok(serde_json::from_slice(account.data.actor_state(actor.program_account_id))?)
}
async fn wait_and_sync(wallet: &mut WalletCore<SequencerClient>, hash: HashType) -> Result<()> {
    let start = Instant::now();
    loop {
        if RpcClient::get_transaction(&wallet.client_owned(), hash).await?.is_some() {
            wallet.sync_to_latest_block().await?;
            return Ok(());
        }
        ensure!(start.elapsed() < Duration::from_secs(240), "transaction inclusion timed out: {hash}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
async fn transaction(wallet: &WalletCore<SequencerClient>, hash: HashType, label: &str) -> Result<Value> {
    let (tx, block_id) = RpcClient::get_transaction(&wallet.client_owned(), hash).await?.context("transaction missing")?;
    let block = RpcClient::get_block(&wallet.client_owned(), block_id).await?.context("block missing")?;
    let index = block.body.transactions.iter().position(|x| x.hash() == hash).context("transaction not in block")?;
    let mut record=json!({"label":label,"tx":hex::encode(hash.0),"block":block_id,"reverted":block.body.is_reverted(index),"transaction":serde_json::to_value(&tx)?});
    if let common::transaction::LeeTransaction::PrivacyPreserving(private) = &tx {
        let raw=private.witness_set.proof().clone().into_inner();
        let inner:risc0_zkvm::InnerReceipt=borsh::from_slice(&raw)?;
        let fake=matches!(&inner,risc0_zkvm::InnerReceipt::Fake{..});
        let receipt=risc0_zkvm::Receipt::new(inner,lee_core::to_borsh_frame(&(&private.message.public_context,&private.message.execution)));
        if std::env::var("RISC0_DEV_MODE").unwrap_or_default()!="1" {
            ensure!(!fake,"non-development transaction carried a fake receipt");
            receipt.verify(lee::PRIVACY_PRESERVING_CIRCUIT_ID)?;
            if let Ok(dir)=std::env::var("FW_PROOF_EVIDENCE_DIR") {
                fs::create_dir_all(&dir)?;
                fs::write(Path::new(&dir).join(format!("{label}-receipt.bincode")),bincode::serialize(&receipt)?)?;
                fs::write(Path::new(&dir).join(format!("{label}-journal.bin")),&receipt.journal.bytes)?;
            }
        }
        record["proof"]=json!({"kind":if fake{"fake"}else{"cryptographic"},"bytes":raw.len(),
            "independently_verified":!fake,"circuit_image_id":risc0_zkvm::sha::Digest::from(lee::PRIVACY_PRESERVING_CIRCUIT_ID).to_string()});
    }
    Ok(record)
}
async fn run(run: &Path, program_path: &Path, witness_path: &Path) -> Result<()> {
    let ids = read(run.join("accounts.json"))?;
    let payer: AccountId = ids["payer"].as_str().unwrap().parse()?;
    let instance: AccountId = ids["instance"].as_str().unwrap().parse()?;
    let verifier: AccountId = ids["verifier"].as_str().unwrap().parse()?;
    let mut wallet = open(run)?;
    wallet.sync_to_latest_block().await?;
    let mut steps = vec![];
    wallet.set_private_fee_payer(Some(verifier));
    let (fund, _) = NativeTokenTransfer(&wallet).transfer(
        AccountIdentity::Public(payer), AccountIdentity::PrivateOwned(verifier),
        100_000_000_000_000u128, CreditDelivery::Automatic).await?;
    wait_and_sync(&mut wallet, fund).await?;
    steps.push(transaction(&wallet, fund, "fund_private_verifier").await?);
    let bytes = fs::read(program_path)?;
    let program = Program::new(bytes.clone().into())?;
    wallet.set_private_fee_payer(None);
    let program_id = program_deployment::deploy_program(&mut wallet, bytes, payer).await?;
    let catalog: ProgramCatalog = [(program_id, program)].into_iter().collect();
    let actor = Actor::new(instance, program_id);
    let (init, _) = wallet.send_tx(
        vec![AccountIdentity::Public(payer).balance(), AccountIdentity::Public(instance).select_program_actor_state(program_id)],
        1, Program::serialize_message(serde_json::to_vec(&json!({"op":"initialize"}))?)?,
        &catalog, None, AsyncDelivery::default()).await?;
    wait_and_sync(&mut wallet, init).await?;
    steps.push(transaction(&wallet, init, "initialize_public_instance").await?);
    let before = snapshot(&wallet, actor).await?;
    write(run.join("state-before.json"), &before)?;
    let witness = read(witness_path)?;
    let verify = json!({"op":"verify","public_instance":instance.to_string(),"witness":witness});
    wallet.set_private_fee_payer(Some(verifier));
    let start = Instant::now();
    let (update, _) = wallet.send_privacy_preserving_tx(
        vec![AccountIdentity::PrivateOwned(verifier).select_program_actor_state(program_id),
             AccountIdentity::PublicNoSign(instance).select_program_actor_state(program_id)],
        0, Program::serialize_message(serde_json::to_vec(&verify)?)?, &catalog).await?;
    wait_and_sync(&mut wallet, update).await?;
    let update_wall_seconds = start.elapsed().as_secs_f64();
    steps.push(transaction(&wallet, update, "private_verify_public_apply").await?);
    let after = snapshot(&wallet, actor).await?;
    ensure!(after != before, "update did not change instance state");
    ensure!(after["updates_applied"] == 1 && after["finalized_slot"] == 7109344 && after["execution_block"] == 17923026,
            "unexpected final instance state");
    write(run.join("state-after.json"), &after)?;
    // An ordinary external root call cannot claim the internal verifier role.
    wallet.set_private_fee_payer(None);
    let forged = json!({"op":"apply","verified":{
        "chain_id":1,"previous_store_commitment":after["store_commitment"],
        "new_store_commitment":"1111111111111111111111111111111111111111111111111111111111111111",
        "finalized_slot":7109400,"execution_block":17923027}});
    let (forged_hash, _) = wallet.send_tx(
        vec![AccountIdentity::Public(payer).balance(), AccountIdentity::Public(instance).select_program_actor_state(program_id)],
        1, Program::serialize_message(serde_json::to_vec(&forged)?)?, &catalog,
        None, AsyncDelivery::default()).await?;
    wait_and_sync(&mut wallet, forged_hash).await?;
    let forged_record = transaction(&wallet, forged_hash, "external_apply_refused_on_node").await?;
    ensure!(forged_record["reverted"] == true, "node accepted forged apply");
    ensure!(snapshot(&wallet, actor).await? == after, "forged apply changed state");
    steps.push(forged_record);
    wallet.set_private_fee_payer(Some(verifier));
    let replay_result = wallet.send_privacy_preserving_tx(
        vec![AccountIdentity::PrivateOwned(verifier).select_program_actor_state(program_id),
             AccountIdentity::PublicNoSign(instance).select_program_actor_state(program_id)],
        0, Program::serialize_message(serde_json::to_vec(&verify)?)?, &catalog).await;
    let replay_error = match replay_result { Ok(_) => anyhow::bail!("a stale update must be refused"), Err(e) => e };
    ensure!(snapshot(&wallet, actor).await? == after, "replay changed public state");
    let result = json!({"mode":"local_LEZ_v0.4.0_rc1_standalone_development_mode",
        "program_account":program_id.to_string(),"instance":instance.to_string(),
        "risc0_dev_mode":std::env::var("RISC0_DEV_MODE").ok(),"update_wall_seconds":update_wall_seconds,
        "sequence_wall_seconds_starting_with_update":start.elapsed().as_secs_f64(),
        "before":before,"after":after,"steps":steps,
        "replay":{"stage":"wallet_preflight_not_submitted","error":format!("{replay_error}"),"state_unchanged":true},
        "limitations":"Local standalone Bedrock mock, development-mode receipts, no real proof or public-testnet claim."});
    write(run.join("result.json"), &result)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("prepare") => prepare(&PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("run") => run(Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4])).await,
        Some("run-sequence") => run_sequence(Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4]),args.get(5).map(|v|v.parse()).transpose()?.unwrap_or(7)).await,
        Some("prove") => prove(Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4])),
        Some("verify-receipt") => {
            ensure!(!risc0_zkvm::ProverOpts::succinct().dev_mode(),"verification refuses development mode");
            let receipt:risc0_zkvm::Receipt=bincode::deserialize(&fs::read(&args[2])?)?;
            ensure!(!matches!(&receipt.inner,risc0_zkvm::InnerReceipt::Fake{..}),"fake receipt");
            receipt.verify(risc0_zkvm::sha::Digest::try_from(hex::decode(&args[3])?).map_err(|_|anyhow::anyhow!("image ID must be 32 bytes"))?)?;
            println!("PASS: cryptographic receipt verifies against the supplied image ID");Ok(())
        },
        _ => anyhow::bail!("usage: driver prepare RUN_DIR LEZ_SOURCE | run RUN_DIR PROGRAM_BIN WITNESS_JSON"),
    }
}
