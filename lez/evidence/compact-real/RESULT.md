# Real-proof result

One complete private committee update was accepted at local LEZ block **1101**.
`RISC0_DEV_MODE` was unset. The actual transaction receipt independently verifies
against official circuit image `8fc948d117c11d55f2036748013b3dc599d6995545f0b020d090f544081cf257`. Wrong-image verification and journal
tampering are rejected. The receipt is `verified_update_0-receipt.bincode`.

Wallet proving through inclusion took **26113.142 seconds
(7.25 hours)** on the recorded two-core CPU environment. This includes the
program proof, privacy-preserving circuit proof and block wait.

This first update installs the next sync committee. Its finalized slot remains
7069376; `updates_applied` changes from 0 to 1. Seven
successive historical updates were separately tested using development receipts.
This does not claim seven real proofs, public-testnet deployment or live tracking.

Transaction: `60573a4fd65253bc7ebd4023294d5d602b0e69630ced87197c18c7788f731163`. Detailed state and raw RPC evidence are in
`sequence-result.json` and `rpc-responses.json`.
