# EdgeRouter scripts (v2)

Maintenance scripts for Ubiquiti EdgeRouter (tested on **ER-6P**). WAN preference / repair logic is in Rust; thin `.sh` wrappers keep cron-friendly names. Site-specific values live in **`config.toml`** (not committed — see `config.example.toml`).

**Target:** EdgeOS userspace is **32-bit big-endian MIPS** (o32), even on Octeon 64-bit CPUs. Binaries must also be **statically linked** (musl) because EdgeOS glibc is too old. A 64-bit or dynamically linked binary fails on the router with `Accessing a corrupted shared library`.

## Layout

| Path | Role |
|------|------|
| `src/*.rs`, `src/bin/*.rs` | Rust library + binaries |
| `src/*.sh` | Wrappers and helper scripts |
| `docker/Dockerfile.er6p` | Cross-compile image (Ubuntu 24.04 + 32-bit mips musl gcc) |
| `build.sh` | Build that image, compile, populate `dist/` |
| `config.example.toml` | Template config (safe to commit) |
| `config.toml` | Your real config (gitignored) |
| `dist/` | Build output — copy to the router |

Deploy path on the router is typically `/config/scripts/` (flat). Place `config.toml` next to the binaries.

## Configuration

```bash
cp config.example.toml config.toml
# edit interfaces, preference order, webhook URLs/keys, paths
```

Important fields:

- **`wan_interfaces`** — preference order (highest first). Each entry has `name` and `kind` (`pppoe` or `ethernet`).
- **`nat_probe_device`** — LAN host used when reading NAT translations.
- **`webhooks`** — log upload + remote command URLs and access keys.
- **`scripts_dir`**, log paths, `conntrack_path`, `runop`, timing delays.

Config search order: `EDGEROUTER_SCRIPTS_CONFIG` → `config.toml` beside the binary → `/config/scripts/config.toml`.

### Remote commands

Allowlisted webhook commands (names follow your configured interfaces):

| Command | Action |
|---------|--------|
| `restart` | Reboot |
| `reset-<iface>` | Bounce that WAN (`reset-pppoe.sh` or `reset-ethernet.sh`) |
| `disable-<iface>` / `enable-<iface>` | Ethernet WANs only |

Example: with `pppoe0` / `pppoe1` / `eth2` configured, `reset-pppoe0` and `disable-eth2` still work.

### Helper scripts

| Script | Usage |
|--------|--------|
| `reset-pppoe.sh` | `reset-pppoe.sh pppoe0` |
| `reset-ethernet.sh` | `reset-ethernet.sh eth2` |
| `disable-ethernet.sh` / `enable-ethernet.sh` | same pattern |

## Prerequisites (Mac cross-compile)

- **Docker Desktop** — installed **and running** (required by `./build.sh`)
- Host Rust is optional, for `cargo test` only

`./build.sh` compiles inside Docker: Ubuntu 24.04 plus the 32-bit musl compiler from `ghcr.io/cross-rs/mips-unknown-linux-musl` (Rust nightly `-Z build-std` for this Tier 3 target).

## Compile (produces `dist/`)

```bash
./build.sh
```

```bash
file dist/auto-reset-pppoe
# expect: ELF 32-bit MSB executable, MIPS, MIPS32 rel2 … statically linked
```

Host check only (Mac binaries, **not** for the router):

```bash
cargo build
cargo test
```

## Deploy

`/config/scripts/` is usually root-owned, so `scp` straight there fails with `Permission denied`. Copy to `/tmp`, then install with sudo:

```bash
scp dist/* user@router:/tmp/
ssh -t user@router 'sudo cp /tmp/auto-reset-pppoe /tmp/upload-reset-logs /tmp/execute-remote-commands /tmp/*.sh /tmp/*.toml /config/scripts/ && sudo chmod +x /config/scripts/* && sudo chmod a-x /config/scripts/*.toml'
```

On the router:

```bash
file /config/scripts/auto-reset-pppoe
# ELF 32-bit MSB … statically linked
/config/scripts/auto-reset-pppoe
```

## What was rewritten

| Entry | Form |
|-------|------|
| `auto-reset-pppoe` | Rust + `.sh` wrapper; uses `ip_test` library |
| `ip_test` | Library only |
| `upload-reset-logs` | Rust + `.sh` wrapper |
| `execute-remote-commands` | Rust + `.sh` wrapper |
