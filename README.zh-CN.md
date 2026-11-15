# Finality Witness — RFP-021 申请演示

已实现同区块最终性/状态证明、原生调用方、真实 RISC0 执行成本、官方密码学加速和连续增量更新。**已在同一个本地 LEZ 0.4 实例从可信 checkpoint 连续完成七次私有 Verify → public Apply，含五次委员会切换。该全链运行使用开发模式 receipt；另一个全新节点已关闭开发模式，完成一条真实委员会更新证明并由节点接受。** 全部数据仍是 2023 年 Capella 历史样本。

## 1. 同一区块闭环

```text
固定历史 Ethereum mainnet checkpoint
 → bootstrap SSZ 分支、512 人委员会
 → 7 次 BLS 更新、委员会轮换、finality 分支
 → finalized beacon header → execution header SSZ 分支
 → block 17,923,026 的 state_root
 → deposit contract 账户 MPT proof
 → 从认证账户解出的 storage_root → 非零 slot 0 MPT proof
```

账户为 `0x00000000219ab540356cbb839cbe05303d7705fa`。归档 RPC 只提供 witness 字节：balance、storageHash、value 都要与认证叶子核对。固定请求/响应、出处、许可及 SHA-256 在 `fixtures/provenance.json`。验证不需要联网、钱包或私钥。

Python 额外验证同一区块一笔真实交易的 SSZ 包含；包含不等于执行成功。旧的不同区块账户样本只作为反例。

## 2. Python 与原生调用方

需要 Python 3.12；首次安装依赖需联网，之后验证可离线：

```sh
./run.sh
.venv/bin/python -m unittest test_state test_consumer -v
.venv/bin/python consumer.py
```

`report.json` 记录完整重放及 12 个反例。18 项 Python 测试已通过。

Rust consumer 内部重新运行验证器，检查固定 chain/account/slot/predicate，最后才原子提交一张演示票据与 nullifier。标识绑定 chain/block hash/account/slot，不包含可变 predicate。坏证明不改状态，正确证明可重试；重复消费、不成立条件和溢出均不发生部分提交。

```sh
cd rust
cargo run --release -p finality-witness-native -- --consumer-demo input.json
```

这是单进程票据计数示例，不是资金、资产铸造、持久化事务或并发保证。它不接受报告 JSON 作为已验证输入。Rust core 有 25 项测试（原 20 项加 5 项增量测试）；新版 compact profile 有 29 项 core 测试和 10 项 LEZ 原生 handler 测试，这些测试不等于节点执行。

## 3. 真实 RISC0 执行与官方加速

共享 `rust/core` 在 guest 内完成 BLS/SSZ/MPT 检查，不信任宿主的成功标志。Runner 使用官方 r0vm 3.0.5 IPC **执行器**，未开启开发模式，也未调用 prover。

| 配置 | 完整 7 次更新＋账户/存储 user cycles |
|---|---:|
| Stock 软件基线 | 1,747,839,425 |
| 官方 BLS 有限域补丁 | 417,787,748 |
| 官方 BLS＋SHA2 补丁、合并 pairing | 231,806,170 |

优化后约 **7.54×** cycle 加速，验证结果相同；坏签名、假存储值在优化 guest 中仍被拒绝。只比较相同历史 workload 的 cycles，不用不同机器负载下的 wall time 代替 proving speedup。

官方固定提交在 `rust/accelerated/PROVENANCE.json`，独立 workspace/lock 保留原 stock 配置。合并 pairing 使用成熟库的 `multi_miller_loop` 与一次 final exponentiation，不跳过验签，也不继承第三方审计结论。

其他 Linux x86_64 机器安装工具链并复现：

```sh
./scripts/setup-risc0.sh
./scripts/run-risc0.sh
./scripts/run-accelerated.sh
```

默认宿主 Rust 1.91.1 保留原 baseline 复现配置；RISC0 Rust 为 r0.1.91.1，SDK/r0vm 为 3.0.5。`FW_HOST_TOOLCHAIN` 可覆盖宿主版本。当前云环境已换成 1.98.1：

```sh
FW_TOOLCHAIN_ROOT=/workspace/work/toolchains FW_HOST_TOOLCHAIN=1.98.1 ./scripts/run-accelerated.sh
```

脚本可通过 `FW_ACCELERATED_TARGET_DIR`、`FW_RUNNER_TARGET_DIR` 复用缓存。首次编译需要网络、系统 C 工具及数 GB 空间；其他系统自动安装未验证。LEZ 本地节点/driver 另外使用宿主 Rust 1.98.1，不包含在上述独立 executor 安装流程中。

已打包测量对应的 ELF、ProgramBinary、image ID、SHA-256 和 reports：

- `artifacts/verifier.{elf,bin}`：stock。
- `artifacts/verifier-accelerated.{elf,bin}`：官方加速与合并 pairing。
- `artifacts/verifier-incremental.{elf,bin}`：单次增量更新。
- `artifacts/manifest.json`、`BENCHMARKS.md`：版本、image、hash 与方法。
- `rust/accelerated/*executor-report.json`、`optimized-comparison-report.json`：实际执行与反例。

`python3 scripts/check-accelerated-results.py` 可检查现有报告的一致性；检查报告本身不会重新证明共识或产生 zk proof。

## 4. 单次增量更新与固定 snapshot

`rust/historical-store.json` 由原可信 checkpoint 加前六次更新实际验签后导出。可用 `export_snapshot` 重建，源码在 `rust/core/src/incremental.rs`。域分离承诺为：

```text
76084610a9e4dfe1aea101c87bf9734b3d7a5a423d0d63f222c2b75ecfdc3aac
```

增量 guest 验证该承诺后，验证第七次 finality update，并检查同区块账户/存储。实际成本 **32,211,104 user cycles**。它超过 LEZ 0.4 公共 **10,000,000** 上限；本项目没有调高上限后声称适配。

这里导入的是**明确固定的历史 snapshot 信任输入**，不是此前验证过的 zk receipt，也不是今天的新鲜检查点。不能让任意调用者自选根或委员会。它演示一次增量更新，不是任意长期实例或自动追链服务。

## 5. 早期 v1 本地 LEZ 集成记录

早期 v1 actor 已按 LEZ `v0.4.0-rc1` 接口编译并通过当时的 7 项原生 handler 测试（原始源码保存在 `legacy/v1-source.zip`）。设计为私有 Verify 完成昂贵检查，再向 public instance 发送同程序 Apply。Apply 必须通过 `from_own_program()`、旧承诺 CAS 和 slot 递增检查，public state 只存小于 1KB 的摘要。

程序已部署到运行中的本地节点，并由真实钱包通过 RPC 发送交易。`lez/evidence/result.json`、RPC 响应、状态快照和筛选日志保留证据：

- Initialize 在本地区块 **175** 成功。
- 有效的私有 Verify → public Apply 在区块 **177** 成功，`updates_applied` 从 0 变为 1，finalized slot 从 **7104096 → 7109344**，Ethereum execution block 从 **17917816 → 17923026**。
- 外部伪造 Apply 在区块 **178** 被节点回滚，更新后的有效状态保持不变。
- 重复更新被钱包 **preflight** 检查拒绝，未提交节点，状态不变；这不称为节点上链回滚。

这是 `v0.4.0-rc1` **standalone、mock Bedrock、`RISC0_DEV_MODE=1`** 的真实节点/钱包执行、消息传递与状态更新证据。其 receipt 是占位符，不能称真实 zk proof、真实共识网络或公开测试网部署。公共 cap 保持 10M，昂贵验证放在私有路径。

独立 executor 对比使用 V1COMPAT 2.2.3；LEZ actor 固定 2.2.2 与真实 actor framing，不把不同 image/kernel 的运行混作同一 benchmark。详细协议见 `rust/lez-actor/README.md`。

## 6. 演示与边界

讲解顺序：历史 checkpoint 与来源 → 同根账户/存储 → stock 与官方加速结果相同 → 坏输入拒绝 → consumer 0→1→1 → 增量更新及明确超限 → 本地开发模式节点的有效更新、伪造 Apply 回滚与 replay preflight 拒绝。

- 仅支持历史 Capella 样本和固定历史时钟；旧 checkpoint 不能安全启动今天的客户端。
- 新写验证编排未审计；Helios 只提供 MIT fixtures，成熟密码学库不使本项目自动获得审计覆盖。
- 无当前分叉完整支持、实时跟链、运营软件、长期历史头累加器和完整排除证明支持。
- 无成功 ETH 转账/收据日志业务、完整隐私路径、生产 checkpoint 策略或独立审计。
- Executor cycles/wall time 不等于 proving time、proof size 或 LEZ gas；未测峰值内存与目标笔记本。
- 七次全链运行使用开发模式；另有一次真实证明通过。没有公开测试网或完整共识网络记录。

代码 MIT + Apache-2.0；上游许可保留。英文申请补充见 `APPLICATION-ADDENDUM.md`。

## 7. 连续更新 v2

新版 actor 从最初的可信 checkpoint 初始化，不再导入第六次更新后的固定快照。
同一个实际 LEZ 实例七次更新全部成功入块，最终 slot 7,109,344、执行区块 17,923,026。
第一次更新只学习 next committee，允许最终性高度不变但要求状态承诺改变；之后发生五次 current committee 切换。
每次 Apply 对公开旧状态承诺做 CAS，拒绝旧快照和重复更新。

当前使用 `artifacts/lez-actor-compact.bin` 和 `rust/compact-sequence.json`；generic v2 和旧 snapshot 二进制、跑分均保留为历史证据。
详见 [连续更新与真实证明](lez/CONTINUOUS-UPDATES.md)，可运行 `lez/run-sequential.sh ... dev` 或 `... real 1`。
`lez/evidence/sequential-dev/` 保存七次节点更新及独立 RPC 记录；真实证明须另看完成后的非开发模式证据，不能把此开发模式全链运行当作真实证明。

委员会根版本已在实际节点完成七次更新，见 `lez/evidence/compact-dev/`。首条更新的实际 RISC0 执行量为 13,977,215 cycles，仍高于公开交易 10M 上限，使用私有 Verify → public Apply。它在 guest 内验证完整 BLS 和未签名成员的 Merkle 分支，不接受 host 的“已验证”标记。

## 长时间 CPU 证明

`scripts/background-proof.py` 可以把一次真实本地更新放入独立后台进程组，记录进度和退出状态。它在节点接收、独立 receipt 验证及负面检查全部通过后才将状态标记为 `complete`，并自动补充证明证据及更新 ZIP。任务失败或被回收会显示 `failed` / `interrupted`，不会被当作证明成功。机器仍需保持运行。

```sh
python scripts/background-proof.py --detach --run-dir /absolute/path/to/fresh-run \
  --node-bin /path/to/sequencer_service --driver-bin /path/to/finality-witness-lez-driver \
  --lez-source /path/to/lez-source --r0vm /path/to/r0vm
python scripts/background-proof.py --run-dir /absolute/path/to/fresh-run --status
```

启动时会冻结该任务的 actor、输入和验证脚本；后台完成后只在文件未被后续编辑时更新已有说明，避免覆盖新改动。运行目录含临时钱包，不能直接作为公开交付目录。

## 真实证明结果

已完成一次完整私有更新的真实证明，并在本地节点区块 1101 被接受。独立 receipt 验证通过，错误 image 与篡改 journal 均被拒绝。整笔钱包证明至入块耗时 7.25 小时（2 核 CPU）。该条只学习下一届委员会，最终性高度不变；七次连续更新记录仍使用开发模式。详见 [证明结果](lez/evidence/compact-real/RESULT.md)。
