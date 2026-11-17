# Public LEZ v0.3 deployment

**Status: deployed and initialised on the public LEZ v0.3 testnet on 2026-11-16.**

This is the public counterpart to the local executor and state-machine evidence in
[`evidence/`](evidence/). It is a third-party-checkable artefact: every value below
can be read back from the public sequencer with `getAccount`.

This deployment does **not** satisfy any requirement that names LEZ v0.4. It is the
v0.3 plan/apply adapter described in [`README.md`](README.md), carrying the same
authenticated historical Capella verification core.

## Identities

| Item | Value |
| --- | --- |
| Program account (loader header) | `5FKkrZ6Fch7zRRNAmUuosidosJoLAgamaEciRNtdAx3x` |
| Instance PDA (seed `[0x46; 32]`) | `3UEzjhNLDdBTpVx5moZdZQGV1efyUpqt9X1gv9cXENHg` |
| Program image ID | `d3493782be7b206ba2303c7102326c5253fe2233b1439d42bad08f283d139a1e` |
| Network | `https://testnet.lez.logos.co` (LEZ v0.3.1) |
| Live circuit image | `189513eb0a037edb638b3da824952e69894a4d11ab05c5956a5986181ab02721` |
| Loader header | block 82771, `immutable` |
| Instance initialised | block 82772 |
| Result | `deployed_and_initialized` |

The program image ID is the one recorded in [`artifacts/manifest.json`](artifacts/manifest.json),
and the archived guest binary's SHA-256 matches that manifest for both the `.bin` and
the `.elf`.

## On-chain instance state

Read from the sequencer and decoded from the program's shard:

```json
{
  "chain_id": 1,
  "execution_block": 17883333,
  "finalized_slot": 7069376,
  "schema": "fw-v03-historical-v1",
  "store_commitment": "b75239acc0beb0174c5fe8beaa0213a5685ead076e730174bde1e77fb4420ab6",
  "updates_applied": 0
}
```

`updates_applied` is **0**. No update has been applied to this instance. The
deployment establishes that the adapter runs on the public network and initialises
from its pinned checkpoint; it does not establish any update result.

## Transactions

Nine transactions, all included:

| Step | Block | Transaction |
| --- | ---: | --- |
| `deploy_segment_9` | 82745 | `e20007148111620b8f4d944d409f3a604ea3c327067afef643203468ce714aea` |
| `deploy_segment_7` | 82758 | `78b782e2b805c5d0f1785e7cdf678f40116a1b2c27ca955b63ab0f2b4a01e6b4` |
| `deploy_segment_6` | 82761 | `c450bfd04aab31c774b9c73728c5d8cec61486afc1eb8fda790baa963f8847c5` |
| `deploy_segment_5` | 82762 | `e51de62647aa6785d156f06f9d2b245f53bc4c3f5d72b8343702ca1a84347b19` |
| `deploy_segment_3` | 82766 | `a5ae5a97fe3f7d59d5fcd1e79ddb657c2e34d6bcc9ae9986ca898cd11ffa1626` |
| `deploy_segment_2` | 82767 | `b42c64c40c6821e6ed1651eed455b39822f46d5e83c97b74ea4918a4c467f246` |
| `deploy_segment_0` | 82770 | `3d21ce5346789cb20d427dda51769667a123909dab94ff1bf973d5d3f75d2697` |
| `deploy_header` | 82771 | `5b2dd136a4b8d763…` |
| `initialize_historical_checkpoint` | 82772 | `5205c216bc0ad562…` |

Ten segments were uploaded in total; the last two transaction hashes are truncated
here and recorded in full in the run directory. All ten segment accounts were
verified to hold nonce = 1 and their shard data before the header was written.

## How to check it

```sh
# Instance state: decode the shard bytes as UTF-8 JSON
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getAccount","params":["3UEzjhNLDdBTpVx5moZdZQGV1efyUpqt9X1gv9cXENHg"],"id":1}'

# Immutable loader header present (nonce 1)
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getAccount","params":["5FKkrZ6Fch7zRRNAmUuosidosJoLAgamaEciRNtdAx3x"],"id":1}'

# Any transaction from the table
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getTransaction","params":["5205c216bc0ad562…"],"id":1}'
```

## Why the deployment took four hours

Not a fault in the program. The v0.3 chain emits blocks slowly and irregularly, and
it has degraded over its lifetime. Decoding the `timestamp` field of the Borsh
`BlockHeader` (`block_id: u64`, `prev_block_hash: [u8;32]`, `hash: [u8;32]`,
`timestamp: u64` in milliseconds; timestamp at byte offset 72) gives:

| Blocks | Date | Mean gap | Median gap |
| --- | --- | ---: | ---: |
| 25700–25707 | 2026-10-16 | 10 s | 10 s |
| 55000–55007 | 2026-10-22 | 11 s | 10 s |
| 76000–76007 | 2026-11-03 | 71 s | 60 s |
| 82700–82759 | 2026-11-16 | **558 s** | 60 s |

The longest single gap in that window was 9,600 s (2 h 40 m) at block 82729, which
occurred before any transaction of this deployment was submitted.

Block size is not the cause. Correlating inter-block time against block size over
blocks 82700–82759 gives **r = +0.033**. The 9,600 s gap carried an empty 714-byte
block, while two 99 KB blocks carrying this deployment's own segments were followed
by 120 s gaps.

The upload is ten sequential segment transactions, each waiting for inclusion before
the next. At 558 s mean spacing that is hours, and the driver's 20-minute
per-transaction wait expires repeatedly. A resumable retry loop that re-reads the
deployment record, skips segments already on chain and reconnects completed the run
in five attempts.

For scale: the earlier public deployment in this round's competing work placed its
transactions at block 25788 on 2026-10-16 22:35 UTC, during the 10 s/block era.

## Faucet behaviour observed

The first claim reported `submitted` with a transaction hash, but that transaction
was absent from the chain and the recipient balance stayed at zero for over twenty
minutes; it appeared later. A second claim on a fresh address sat at `queued` for
thirteen minutes before behaving the same way. The hosted faucet's confirmation
state lags the chain in the same way everything else here does.

## Limitations

- LEZ **v0.3**, not v0.4. Requirements naming testnet 0.4 are not satisfied.
- Historical Capella checkpoint only. No live following, no current-fork support.
- `updates_applied` is 0: no update has run on this instance.
- No real v0.3 proving receipt is claimed. The v0.4 receipt in
  [`../evidence/compact-real/`](../evidence/compact-real/) does not transfer.
- The driver was built on macOS arm64 rather than Linux, so the build path here
  differs from the pinned Linux toolchain used for the archived guest binary. The
  deployed binary is the archived one, checked against its manifest hash; the
  driver's platform does not affect it.
