# Public testnet deployment status

**Not deployed.** The public endpoint was reachable on 2026-11-15, but its circuit fingerprint does not match this repository's archived LEZ v0.4.0-rc1 wallet/program integration. No wallet was imported, faucet claim made, or transaction submitted during this check.

## Observed compatibility

| Item | Archived real-proof stack | Public endpoint |
| --- | --- | --- |
| Network | Local standalone v0.4.0-rc1, mock Bedrock | `https://testnet.lez.logos.co` |
| Privacy-preserving circuit | `8fc948d117c11d55f2036748013b3dc599d6995545f0b020d090f544081cf257` | `189513eb0a037edb638b3da824952e69894a4d11ab05c5956a5986181ab02721` |
| Interface used by this actor | `ReceiveInput`, `Response`, synchronous same-program messages | Official public-testnet documentation identifies the v0.3 runtime, whose program interface is plan/apply |
| Deployment of this prototype | One accepted real local update | **Not performed** |

The raw read-only health, height, program ID, fee-market and channel responses, their timestamp, and the comparison outcome are archived in [preflight.json](evidence/public-testnet/preflight.json). At the recorded check the height was 82,463 and the public execution cap was 10,000,000. A reachable endpoint and a matching chain/channel name do not establish ABI compatibility.

## Sources checked

- [Official deployment documentation](https://docs.logos.co/lez/programs/write-and-deploy-lez-program-with-scaffold#deploy-to-the-testnet) identifies the current public network as v0.3 and requires a matching wallet and guest release.
- [Logos faucet network contract](https://github.com/logos-co/lez-faucet/blob/228d6b3e3242d46add840882d611ca0bc7b85e78/README.md) identifies `https://testnet.lez.logos.co` as v0.3.1. The corresponding [network documentation](https://github.com/logos-co/lez-faucet/blob/main/docs/testnet.md) records the exact circuit fingerprint observed here.
- [Official v0.4 planned scope, published 2026-11-13](https://roadmap.logos.co/testnets/v04), lists the transactional actor model and in-proof private transaction fees as planned features. A v0.4 source release candidate is not evidence that the public endpoint has upgraded.
- [Current actor source](../rust/lez-actor/src/lib.rs) and its [pinned LEZ dependency](../rust/lez-actor/Cargo.toml) use the v0.4 actor API. The [local proof summary](evidence/compact-real/summary.json) records the circuit that actually accepted the archived transaction.

## Repeat the read-only check

Python 3.12 and HTTPS access to the RPC are sufficient; no wallet is needed:

```sh
python3 scripts/check-public-testnet.py --out /tmp/finality-witness-testnet-preflight.json
```

Exit 2 means the check is blocked by a fingerprint mismatch or RPC failure. Exit 0 means only that the fingerprint matches; it does not establish a deployment, working ABI, available funds or successful update. The check exposes no transaction-submission operation.

A different endpoint can be checked with `--rpc`. The expected fingerprint is read from the completed real-proof summary, rather than edited to match the server. All testnet evidence must continue to distinguish public network observations from local integration evidence.

## What an actual public deployment still needs

For the existing v0.4 actor, obtain a public v0.4 endpoint and verify its wallet/runtime compatibility, then create a separate testnet wallet, acquire test tokens, deploy the exact program, and independently confirm included non-reverted transactions and resulting actor state. Private proving must use that network's actual circuit. The archived local receipt cannot be replayed as a new public transaction.

For a demonstration on the current public v0.3 network, a **separate v0.3 plan/apply program and matching driver** have been implemented and locally verified. See [the isolated adapter](v03/README.md) and [current funding/deployment status](v03/evidence/deployment-status.json). No public deployment is claimed: the newly created payer is unfunded and the hosted faucet could not be reached from this environment. Reuse of the Ethereum core does not remove the need to verify the new authorization and state-transition boundaries, fee behavior and execution cap. The existing v0.4 proof would not validate this different program. Such a deployment would demonstrate v0.3 support; it would not fulfill RFP-021's specified public v0.4 testnet requirement.

The Ethereum workload remains historical Capella unless current-fork and fresh-checkpoint support is separately added. Putting an archived historical verifier on a public LEZ chain would not establish live Ethereum following.
