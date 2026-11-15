#!/usr/bin/env python3
"""Run and finalize one real local LEZ update in a detached process group.

Completion requires node inclusion, a non-fake receipt, independent verification,
wrong-image/journal-tamper rejection and a validated deliverable archive.
"""
import argparse
import datetime
import fcntl
import hashlib
import json
import os
import pathlib
import signal
import shutil
import socket
import subprocess
import sys
import threading
import time
import traceback
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def write_json(path, value):
    path = pathlib.Path(path)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, indent=2) + "\n")
    tmp.replace(path)


def process_table():
    table = {}
    for path in pathlib.Path("/proc").iterdir():
        if path.name.isdigit():
            try:
                fields = (path / "stat").read_text().split()
                table[int(path.name)] = dict(pid=int(path.name), ppid=int(fields[3]),
                    group=int(fields[4]), state=fields[2], name=fields[1].strip("()"),
                    cpu_ticks=int(fields[13]) + int(fields[14]),
                    rss=int(fields[23]) * os.sysconf("SC_PAGE_SIZE"))
            except (OSError, ValueError, IndexError):
                pass
    return table


def monitor(run, proof_pid, stop, started):
    peak = 0
    while not stop.wait(5):
        table = process_table()
        ids = {proof_pid}
        while True:
            new = ids | {pid for pid, row in table.items() if row["ppid"] in ids}
            if new == ids:
                break
            ids = new
        processes = [table[pid] for pid in ids if pid in table]
        rss = sum(row["rss"] for row in processes)
        peak = max(peak, rss)
        trace = []
        for directory in pathlib.Path("/tmp").glob(".tmp*"):
            try:
                files = list(directory.glob("*.bincode"))
                if files and min(file.stat().st_mtime for file in files) >= started:
                    loaded = [int(file.stem) for file in files if file.stem.isdigit()
                              and file.stat().st_atime > file.stat().st_mtime + 1]
                    trace.append(dict(directory=directory.name, segments=len(files),
                                      last_segment_loaded=max(loaded, default=0)))
            except OSError:
                pass
        disk = os.statvfs(run)
        row = dict(updated_utc=utc(), elapsed_seconds=time.time() - started,
                   processes=processes, trace=trace, tree_rss_bytes=rss,
                   sampled_peak_tree_rss_bytes=peak,
                   free_disk_bytes=disk.f_bavail * disk.f_frsize)
        write_json(run / "progress.json", row)
        with (run / "resource-samples.jsonl").open("a") as stream:
            stream.write(json.dumps(row) + "\n")


def worker(args):
    run = args.run_dir
    env = os.environ.copy()
    env.pop("RISC0_DEV_MODE", None)
    env.update(RISC0_SERVER_PATH=str(args.r0vm), RISC0_EXECUTOR="ipc", RISC0_PROVER="ipc",
               RISC0_FEATURE_bigint2="1", SUPPRESS_VERBOSE_PRINTS="1", RAYON_NUM_THREADS="2",
               RUST_LOG="info", FW_PROOF_EVIDENCE_DIR=str(run / "proof-evidence"))
    env["FW_PROOF_PROJECT_ROOT"] = str(ROOT)
    base = dict(supervisor_pid=os.getpid(), started_utc=utc(), run_directory=str(run),
                actor_image_id="35bbc235664c1a6adb0d41c93b2f49f2af7619371a615609a06a40aa327a1bf8")
    node = proof = None
    stop = threading.Event()
    thread = None

    def status(state, **fields):
        write_json(run / "status.json", dict(base, state=state, updated_utc=utc(), **fields))

    def terminated(signum, _frame):
        raise RuntimeError(f"Supervisor received signal {signum}; no completion is inferred")

    signal.signal(signal.SIGTERM, terminated)
    signal.signal(signal.SIGINT, terminated)
    try:
        lock = (ROOT / ".background-proof.lock").open("w")
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        probe = socket.socket()
        try:
            if probe.connect_ex(("127.0.0.1", 3040)) == 0:
                raise RuntimeError("RPC port 3040 is occupied; refusing to use another node")
        finally:
            probe.close()
        inputs = run / "inputs"
        inputs.mkdir()
        for relative in ("artifacts/lez-actor-compact.bin", "rust/compact-sequence.json"):
            shutil.copyfile(ROOT / relative, inputs / pathlib.Path(relative).name)
        logic = inputs / "logic"
        logic.mkdir()
        for name in ("collect-node-evidence.py", "check-sequence.py", "finalize-real-proof.py", "render-benchmarks.py"):
            shutil.copyfile(ROOT / "scripts" / name, logic / name)
        paths = ["README.md", "APPLICATION-ADDENDUM.md", "lez/CONTINUOUS-UPDATES.md",
                 "rust/core/src/lib.rs", "rust/core/src/incremental.rs", "rust/core/src/compact.rs",
                 "rust/lez-actor/src/lib.rs", "rust/lez-actor-fast/Cargo.toml", "rust/lez-actor-fast/Cargo.lock"]
        write_json(run / "launch-snapshot.json", dict(created_utc=utc(),
            sha256={name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in paths},
            actor_sha256=hashlib.sha256((inputs / "lez-actor-compact.bin").read_bytes()).hexdigest(),
            sequence_sha256=hashlib.sha256((inputs / "compact-sequence.json").read_bytes()).hexdigest()))
        status("preparing")
        subprocess.run([str(args.driver_bin), "prepare", str(run), str(args.lez_source)],
                       env=env, check=True)
        config_path = run / "local-demo.json"
        config = json.loads(config_path.read_text())
        config["block_create_timeout"] = "24s"  # Validated limit is < posting_timeout=25s.
        write_json(config_path, config)
        with (run / "setup.log").open("w") as log:
            subprocess.run([str(args.node_bin), "setup", str(run / "node"), "--config", str(config_path),
                "--create-channel", "--funding-public-key",
                "2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26"],
                env=env, check=True, stdout=log, stderr=subprocess.STDOUT)
        with (run / "node.log").open("w") as log:
            node = subprocess.Popen([str(args.node_bin), "start", str(run / "node"),
                "--rpc-listen-address", "127.0.0.1:3040"], env=env, stdin=subprocess.DEVNULL,
                stdout=log, stderr=subprocess.STDOUT, close_fds=True)
        (run / "node.pid").write_text(str(node.pid) + "\n")
        ready = False
        for _ in range(120):
            if node.poll() is not None:
                raise RuntimeError("Local node exited during startup; see node.log")
            try:
                request = urllib.request.Request("http://127.0.0.1:3040",
                    data=b'{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}',
                    headers={"Content-Type": "application/json"})
                if "result" in json.load(urllib.request.urlopen(request, timeout=2)):
                    ready = True
                    break
            except OSError:
                pass
            time.sleep(1)
        if not ready:
            raise RuntimeError("Node RPC startup timed out")
        with (run / "driver.log").open("w") as log:
            proof = subprocess.Popen([str(args.driver_bin), "run-sequence", str(run),
                str(inputs / "lez-actor-compact.bin"), str(inputs / "compact-sequence.json"), "1"],
                env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, close_fds=True)
        (run / "driver.pid").write_text(str(proof.pid) + "\n")
        started = time.time()
        thread = threading.Thread(target=monitor, args=(run, proof.pid, stop, started), daemon=True)
        thread.start()
        phase = "deploying"
        status(phase, driver_pid=proof.pid, node_pid=node.pid)
        while proof.poll() is None:
            if phase == "deploying" and "FW_SEQUENCE_BEGIN" in (run / "driver.log").read_text():
                phase = "proving"
                status(phase, driver_pid=proof.pid, node_pid=node.pid)
            time.sleep(5)
        if proof.returncode != 0:
            raise RuntimeError(f"Wallet/prover exited with code {proof.returncode}; see driver.log")
        status("verifying", driver_pid=proof.pid, node_pid=node.pid)
        with (run / "verification.log").open("w") as log:
            subprocess.run([sys.executable, str(logic / "collect-node-evidence.py"),
                "--run", str(run), "--out", str(ROOT / "lez/evidence/compact-real"),
                "--driver", str(args.driver_bin), "--rpc", "http://127.0.0.1:3040"],
                env=env, check=True, stdout=log, stderr=subprocess.STDOUT)
            subprocess.run([sys.executable, str(logic / "check-sequence.py"),
                str(run / "sequence-result.json"), str(inputs / "compact-sequence.json")],
                env=env, check=True, stdout=log, stderr=subprocess.STDOUT)
        subprocess.run([sys.executable, str(logic / "finalize-real-proof.py"), "--run-dir", str(run)], env=env, check=True)
        result = json.loads((run / "sequence-result.json").read_text())
        step = result["steps"][1]
        status("complete", receipt_verified=True, accepted_block=step["block"],
               private_update_wall_seconds=step["wall_seconds"],
               archive=str(ROOT.parent / "finality-witness-demo.zip"))
    except BaseException as error:
        status("failed", error=str(error), receipt_verified=False)
        traceback.print_exc()
    finally:
        stop.set()
        if thread:
            thread.join(timeout=10)
        if node and node.poll() is None:
            node.send_signal(signal.SIGINT)
            try:
                node.wait(timeout=30)
            except subprocess.TimeoutExpired:
                node.terminate()
        # Kill only remaining processes in this supervisor's own detached process group.
        group = os.getpgrp()
        for pid, row in process_table().items():
            if pid != os.getpid() and row["group"] == group and row["state"] != "Z":
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", type=pathlib.Path, required=True)
    parser.add_argument("--node-bin", type=pathlib.Path)
    parser.add_argument("--driver-bin", type=pathlib.Path)
    parser.add_argument("--lez-source", type=pathlib.Path)
    parser.add_argument("--r0vm", type=pathlib.Path)
    parser.add_argument("--detach", action="store_true")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--status", action="store_true")
    args = parser.parse_args()
    args.run_dir = args.run_dir.resolve()
    if args.status:
        value = json.loads((args.run_dir / "status.json").read_text())
        pid = value["supervisor_pid"]
        alive = process_table().get(pid, {}).get("state") not in (None, "Z")
        if value["state"] not in ("complete", "failed") and not alive:
            value["state"] = "interrupted"
        if (args.run_dir / "progress.json").exists():
            value["progress"] = json.loads((args.run_dir / "progress.json").read_text())
        print(json.dumps(value, indent=2))
        return
    for name in ("node_bin", "driver_bin", "lez_source", "r0vm"):
        value = getattr(args, name)
        if value is None or not value.exists():
            parser.error(f"--{name.replace('_', '-')} must name an existing path")
        setattr(args, name, value.resolve())
    if args.worker:
        worker(args)
    else:
        if args.run_dir.exists():
            parser.error("Use a fresh run directory; refusing to replace a wallet")
        args.run_dir.mkdir(parents=True, mode=0o700)
        command = [sys.executable, str(pathlib.Path(__file__).resolve()), "--worker",
            "--run-dir", str(args.run_dir), "--node-bin", str(args.node_bin),
            "--driver-bin", str(args.driver_bin), "--lez-source", str(args.lez_source), "--r0vm", str(args.r0vm)]
        with (args.run_dir / "supervisor.log").open("w") as log:
            child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                                     start_new_session=True, close_fds=True)
        write_json(args.run_dir / "launcher.json", dict(pid=child.pid, launched_utc=utc(), command=command))
        print(json.dumps(dict(state="launched", pid=child.pid, run_directory=str(args.run_dir))))


if __name__ == "__main__":
    main()
