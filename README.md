# Finality Witness

Ethereum finality and state-attestation prototype for [Logos RFP-021](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-021-ethereum-state-attestation.md), with a stateful LEZ v0.4.0-rc1 actor and archived, independently verified real-proof evidence.

[Application addendum](APPLICATION-ADDENDUM.md) · [Real-proof result](lez/evidence/compact-real/RESULT.md) · [Measurements](BENCHMARKS.md)

## Verified results

| Run | Evidence | Scope |
| --- | --- | --- |
| One complete real private update | [Result](lez/evidence/compact-real/RESULT.md), [summary](lez/evidence/compact-real/summary.json), [offline checks](lez/evidence/compact-real/offline-verification.json) | Wallet and local node with `RISC0_DEV_MODE` unset; private Verify → public Apply accepted at local block **1101**. Original receipt verifies; wrong image and tampered journal are rejected. |
| Seven successive historical updates | [Compact run](lez/evidence/compact-dev/sequence-result.json), [RPC evidence](lez/evidence/compact-dev/rpc-responses.json) | One actual local LEZ instance; five current-committee rotations; reaches finalized slot **7,109,344**, execution block **17,923,026**. These receipts use **development mode**. |
| Same-block account and storage proof | [Fixtures and provenance](fixtures/provenance.json), [verification report](report.json) | Deposit-contract account and nonzero storage slot authenticated under the finalized execution state root; BLS, SSZ and MPT checks. |
| Compact actor execution | [Measurements](lez/CONTINUOUS-UPDATES.md#compact-committee-profile) | **13,977,215 user cycles** for the first committee-only update, compared with 37,044,569 for the generic full-store actor on the same update. Execution cycles are not proving time. |

The real update took **26,113.142 seconds (7.25 hours)** from wallet proving to inclusion on the recorded two-core CPU environment. It installs the next sync committee; finalized height remains unchanged. Only **one** update has a real proof. The seven-update sequence, later finality advances and final account/storage check are separately demonstrated with development receipts.

All node runs are **local standalone LEZ v0.4.0-rc1 with mock Bedrock**, not the public testnet or a full consensus network. Ethereum inputs are recorded **2023 Capella** data, not live chain following.

## Verification flow

```text
Pinned Ethereum mainnet checkpoint
  → authenticated bootstrap and sync committee
  → BLS-verified updates and committee transitions
  → finalized Beacon header
  → authenticated execution header and state root
  → account MPT proof
  → authenticated storage root and storage MPT proof
```

The LEZ actor verifies expensive work privately, then calls public Apply. Apply accepts only its own program and compares the predecessor commitment with current public state before advancing it. The compact profile authenticates committee roots and aggregate keys, checks Merkle witnesses for exactly the non-signers, and verifies BLS signatures inside the guest. It does not accept a host-provided validity flag.

A native consumer separately verifies a claim and atomically grants one demonstration ticket with a nullifier. This consumer is a single-process example; it is not deployed on LEZ and does not handle assets.

## Run the Python demonstration

Python 3.12 and network access for the first dependency installation are required. Fixture verification and replay then run offline.

```sh
git clone https://github.com/JackCC703/ethereum-finality-witness.git
cd ethereum-finality-witness
./run.sh
.venv/bin/python -m unittest test_state test_consumer -v
.venv/bin/python consumer.py
```

`run.sh` checks fixture hashes, verifies the historical chain and state proof, and writes `report.json`. The Python suite contains 18 tests. Input sources, upstream attribution and hashes are in [fixtures/provenance.json](fixtures/provenance.json).

## Inspect or reproduce LEZ evidence

Start with [stateful updates and real proving](lez/CONTINUOUS-UPDATES.md). The current actor is `artifacts/lez-actor-compact.bin`; its input sequence is `rust/compact-sequence.json`. Reproduction requires the official LEZ source, native dependencies, RISC Zero r0vm 3.0.5, and substantial build/proving resources.

```sh
export RISC0_SERVER_PATH=/absolute/path/to/r0vm
# Seven local updates using development receipts:
bash lez/run-sequential.sh /absolute/path/to/lez-source /absolute/path/to/fresh-dev dev
# One complete real private transaction; CPU intensive:
bash lez/run-sequential.sh /absolute/path/to/lez-source /absolute/path/to/fresh-real real 1
```

After building the included [LEZ driver](lez/driver/src/main.rs) as described in the integration documentation, verify the archived receipt without rerunning the prover:

```sh
env -u RISC0_DEV_MODE /path/to/finality-witness-lez-driver verify-receipt \
  lez/evidence/compact-real/verified_update_0-receipt.bincode \
  8fc948d117c11d55f2036748013b3dc599d6995545f0b020d090f544081cf257
```

That image ID identifies the official privacy-preserving circuit. The actor has its own image ID, recorded in [summary.json](lez/evidence/compact-real/summary.json). The driver refuses fake receipts. Node RPC responses, transaction IDs, state commitments and negative verification outcomes are archived beside the receipt.

The first post-processing attempt incorrectly assumed journal bytes occurred once in the serialized receipt. The existing proof and node transaction had succeeded. The collector was corrected and the original receipt reverified, without rerunning the proof or changing the actor. The original failure and recovery are recorded in [verification-recovery.json](lez/evidence/compact-real/verification-recovery.json).

## Repository layout

| Path | Contents |
| --- | --- |
| `rust/core/` | Shared verification logic and compact state profile |
| `rust/lez-actor/`, `rust/lez-actor-fast/` | Stateful LEZ actor and pinned accelerated build |
| `lez/driver/`, `lez/run-sequential.sh` | Actual wallet/node integration and reproduction |
| `lez/evidence/compact-real/` | One accepted real update, receipt and independent checks |
| `lez/evidence/compact-dev/` | Seven-update compact integration run with development receipts |
| `fixtures/` | Historical Ethereum inputs, provenance and upstream licence |
| `artifacts/` | Archived ELF/ProgramBinary files, image IDs and hashes |
| `scripts/` | Build, measurement and evidence collection tools |
| `legacy/`, other evidence directories | Retained earlier source and benchmark snapshots |

The current compact profile passed 29 core tests and 10 native actor tests; the default core passed 25 tests. Native tests, guest execution, development-mode integration and real cryptographic proving are distinct kinds of evidence. Earlier snapshot-based actor artifacts remain for provenance and must not be confused with the current stateful actor.

## Limitations

- Historical Capella profile only. No current-fork completeness, fresh-checkpoint policy, live follower or operational update daemon. The historical checkpoint must not bootstrap a live client today.
- No public-testnet deployment, consensus-network run, deployed consumer, production bridge or independent audit.
- No claim that all seven updates or the final account/storage attestation have real proofs.
- The compact verification workload exceeds the public 10M execution cap and therefore uses private Verify → public Apply. The cap was not increased.
- Local reproduction uses publicly known deterministic test keys and official local genesis accounts. Runtime wallet storage, private node keys and node data are excluded. Never use these test keys for real assets.
- The native ticket consumer does not establish a full privacy-preserving consumer application. Transaction inclusion alone does not establish successful execution.

See [continuous-update integration](lez/CONTINUOUS-UPDATES.md) and [benchmarks](BENCHMARKS.md) for exact methods and boundaries.

## Licence

Original code: **MIT OR Apache-2.0**. See [LICENSE](LICENSE), [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE). Upstream fixtures and dependencies retain their own licences and attribution. Reusing audited primitives does not make this new integration audited.
