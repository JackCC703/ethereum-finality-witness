#!/usr/bin/env python3
import hashlib
import json
import platform
import os
from pathlib import Path

root = Path(os.environ.get('FW_PROOF_PROJECT_ROOT', Path(__file__).resolve().parent.parent))
execution = json.loads((root / 'rust/executor-report.json').read_text())
journal = json.loads(execution['journal'])
metrics = journal['phase_measurements']
rows = []
previous = next(m['cycle_count'] for m in metrics if m['phase'] == 'bootstrap_complete')
rows.append(('Bootstrap (after input decoding)', next(m['delta_cycles'] for m in metrics if m['phase'] == 'bootstrap_complete')))
for i in range(7):
    complete = next(m['cycle_count'] for m in metrics if m['phase'] == f'update_{i}_complete')
    rows.append((f'Update {i}, entire verification', complete - previous))
    previous = complete
rows.append(('Account + one storage proof', metrics[-1]['delta_cycles']))
bls = sum(m['delta_cycles'] for m in metrics if m['phase'].endswith('_after_bls'))
hardware = {
    'platform': platform.platform(),
    'cpu_model': next((l.split(':', 1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')), 'unknown'),
    'cpu_quota': Path('/sys/fs/cgroup/cpu.max').read_text().strip(),
    'memory_limit_bytes': Path('/sys/fs/cgroup/memory.max').read_text().strip(),
    'peak_memory_measured': False,
}
artifacts = {name: hashlib.sha256((root / name).read_bytes()).hexdigest()
             for name in ('artifacts/verifier.elf', 'artifacts/verifier.bin', 'rust/input.json', 'rust/Cargo.lock', 'rust/runner/Cargo.lock')}
manifest_path = root / 'artifacts/manifest.json'
manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
manifest.update({'image_id': execution['image_id'], 'hardware': hardware})
manifest.setdefault('sha256', {}).update(artifacts)
for name in ('artifacts/verifier-accelerated.elf', 'artifacts/verifier-accelerated.bin',
             'artifacts/verifier-incremental.elf', 'artifacts/verifier-incremental.bin',
             'artifacts/lez-actor.elf', 'artifacts/lez-actor.bin',
             'rust/accelerated/Cargo.lock', 'rust/accelerated/bls-only/Cargo.lock',
             'rust/lez-actor/Cargo.lock', 'rust/historical-store.json', 'rust/incremental-input.json'):
    if (root / name).exists():
        manifest['sha256'][name] = hashlib.sha256((root / name).read_bytes()).hexdigest()
for variant, report_path, prefix in (
    ('accelerated', 'rust/accelerated/optimized-executor-report.json', 'verifier-accelerated'),
    ('incremental', 'rust/accelerated/incremental-executor-report.json', 'verifier-incremental')):
    if (root / report_path).exists():
        observed = json.loads((root / report_path).read_text())
        manifest.setdefault('variants', {}).setdefault(variant, {}).update({
            'image_id': observed['image_id'], 'image_id_source': report_path,
            'kernel': 'risc0-zkos-v1compat 2.2.3', 'user_cycles': observed['user_cycles'],
            'mode': 'executor_only_no_zk_proof', 'elf': f'artifacts/{prefix}.elf',
            'program_binary': f'artifacts/{prefix}.bin'})
(root / 'artifacts/manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
lines = ['# RISC0 execution baseline', '',
         'Measured by the actual RISC Zero 3.0.5 executor. No prover was called and no proof/receipt was generated.', '',
         f"- Image ID: `{execution['image_id']}`",
         f"- Total executor user cycles: **{execution['user_cycles']:,}**",
         f"- Segments at po2=20: {execution['segment_count']:,}",
         f"- Executor wall time: {execution['wall_seconds']:.3f} seconds (NOT proving time)",
         f'- Machine: {hardware["cpu_model"]}; CPU quota `{hardware["cpu_quota"]}`; memory limit {hardware["memory_limit_bytes"]} bytes.',
         '- Peak memory and target-laptop performance were not measured.', '',
         'The workload starts from the pinned historical checkpoint, verifies all seven updates, and then verifies the deposit-contract account and slot 0 under finalised block 17,923,026.', '',
         '| Phase | Guest cycle delta |', '| --- | ---: |']
lines += [f'| {phase} | {cycles:,} |' for phase, cycles in rows]
lines += ['', f'BLS signature sections together: **{bls:,}** guest cycles.', '',
          'Method: phase boundaries use guest `env::cycle_count()`. They include bookkeeping between adjacent boundaries; they are not separately executed LEZ transactions. Input parsing, instrumentation, serialization and execution overhead explain differences from total executor user cycles.', '',
          'This is a **stock `bls12_381` software baseline**, with two pairing evaluations per signature and no RISC0 cryptographic acceleration. It replays consensus history from scratch. It is not comparable to a competitor\'s incremental, accelerated single update by dividing totals or comparing wall time.', '',
          'This stock full replay is not a single LEZ transaction. See the separately measured acceleration and incremental sections below; neither full replay nor incremental execution fits the 10M public cap.', '',
          'Negative inputs are also executed in the same guest. The native consumer independently checks proof validity before committing state. See reports and `scripts/check-results.py`.', '',
          'Toolchains: Rust 1.91.1; RISC0 Rust r0.1.91.1; SDK/r0vm 3.0.5. Dependency lockfiles and artifact hashes are in `artifacts/manifest.json`.']
if (root / 'rust/accelerated/optimized-executor-report.json').exists():
    optimized = json.loads((root / 'rust/accelerated/optimized-executor-report.json').read_text())
    incremental = json.loads((root / 'rust/accelerated/incremental-executor-report.json').read_text())
    first = json.loads((root / 'rust/accelerated/executor-report.json').read_text())
    assert json.loads(optimized['journal'])['result'] == journal['result']
    lines += ['', '## Official acceleration and equivalent pairing optimization', '',
              '| Configuration | Seven-update executor user cycles |', '| --- | ---: |',
              f"| Original stock baseline | {execution['user_cycles']:,} |",
              f"| Official BLS finite-field patch only | {first['user_cycles']:,} |",
              f"| Official BLS + SHA2 patches, combined pairing | {optimized['user_cycles']:,} |", '',
              f"Optimized cycle speedup: **{execution['user_cycles']/optimized['user_cycles']:.3f}x**. Verified result is identical.",
              'Pinned official sources and licences are recorded in rust/accelerated/PROVENANCE.json. The combined pairing uses upstream multi_miller_loop and one final_exponentiation; it does not skip BLS verification.',
              'The original stock workspace remains unpatched. Round-one configuration is retained in rust/accelerated/bls-only/. Bad-signature and false-storage witnesses are rejected by the optimized guest.', '',
              '## One incremental historical update', '',
              f"- Actual executor user cycles: **{incremental['user_cycles']:,}** (includes matching account/storage verification).",
              f"- Image ID: `{incremental['image_id']}`",
              '- The snapshot is rebuilt by verifying the original bootstrap and six updates, then the guest verifies the seventh/finality update.',
              '- Snapshot commitment: `76084610a9e4dfe1aea101c87bf9734b3d7a5a423d0d63f222c2b75ecfdc3aac`.',
              '- Importing this pinned historical snapshot is an explicit trust boundary, not a verified prior zk receipt or a fresh checkpoint.',
              '- This exceeds LEZ v0.4 public MAX_GAS_EXEC=10,000,000. The node cap was not increased. Private Verify -> public Apply is the intended local placement.', '',
              'All standalone comparisons use V1COMPAT kernel 2.2.3. The separate LEZ actor targets 2.2.2 and real actor framing. A different kernel/image is not silently treated as the same benchmark.',
              'Separate local LEZ evidence in lez/evidence/result.json: init block175, private Verify -> public Apply block177, external forged Apply reverted block178; replay refused in wallet preflight before submission. Slot7104096 ->7109344 and execution block17917816 ->17923026. This uses standalone mock Bedrock and dev-mode placeholder receipts, not real zk proofs or a public testnet. These node results are separate from executor measurements.',
              'No proving time, zk receipt, proof size or LEZ gas follows from executor cycles.']
compact_comparison = root / 'rust/lez-actor-fast/evidence/comparison.json'
if compact_comparison.exists():
    compact = json.loads(compact_comparison.read_text())
    lines += ['', '## Stateful actor and compact committee profile', '',
              'Seven historical updates settled on one actual local LEZ instance, including committee transitions. The compact run uses development receipts; see lez/evidence/compact-dev/.', '',
              '| First historical update, actor framing, po2=18 | User cycles |', '| --- | ---: |',
              f"| Generic full store | {compact['generic_first_update_cycles']:,} |",
              f"| Official blst, full store | {compact['blst_full_store_first_update_cycles']:,} |",
              f"| Official blst, compact authenticated store | {compact['compact_first_update_cycles']:,} |", '',
              'All seven Ethereum verification results match the full-store reference. Committee aggregate roots and exact non-signer Merkle witnesses remain verified inside the guest.',
              'These are executor measurements, not proving times or transaction gas. The first update learns the next committee without advancing finality. See lez/CONTINUOUS-UPDATES.md.']
real_result = root / 'lez/evidence/compact-real/sequence-result.json'
if real_result.exists():
    real = json.loads(real_result.read_text())
    if real.get('risc0_dev_mode') is None and real.get('replay', {}).get('state_unchanged'):
        step = real['steps'][1]
        assert step['proof']['kind'] == 'cryptographic' and step['proof']['independently_verified']
        lines += ['', '## Complete real private transaction', '',
                  f"A real private committee update was accepted by the standalone node at block {step['block']}. RISC0_DEV_MODE was unset on wallet/node. Its whole wallet proving-to-inclusion time was {step['wall_seconds']:.3f} seconds on the recorded hardware.",
                  'This includes the actor proof, privacy-preserving circuit proof and block wait; it is not a guest-only proving measurement.',
                  'The archived receipt independently verifies against the official circuit image. Wrong-image and journal-tamper checks fail. Only this one update was run with real proofs; the seven-update run uses development receipts. Local standalone mock Bedrock, not public testnet.']
(root / 'BENCHMARKS.md').write_text('\n'.join(lines) + '\n')
print('Wrote BENCHMARKS.md and artifacts/manifest.json')
