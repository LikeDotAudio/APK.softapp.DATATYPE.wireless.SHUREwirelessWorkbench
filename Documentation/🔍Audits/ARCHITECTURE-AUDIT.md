# 🔍 Architecture audit — DATATYPE.wireless.SHUREwirelessWorkbench (2026-10-30)

Audited against the APK.audio multi-repo architecture standard: coupling, boundaries, duplication, contracts.
← [Audits](INDEX.md) · Improvements: [🛣Roadmap/IMPROVEMENTS.md](../%F0%9F%9B%A3Roadmap/IMPROVEMENTS.md)

## Bounded context
The Shure Wireless Workbench `.shw` show-file dialect: a Rust parser (`wwb_importer.rs`) and the `Worker-WIRELESS_WWB` container that answers jobs on the bus with parsed channels, plus a legacy Python importer edge (`IMPORT:file:SHOW/Entry.py`). ~920 lines total; Rust 2024 + rumqttc + zip.

Sampled: every tracked file (small repo) — `Docker/`, `SRC/Cargo.toml`, `SRC/config.ini`, `SRC/src/lib.rs`, `self_test.rs`, the bin, `wwb_importer.rs`, `test_wwb_importer.rs`, `Entry.py`, `display.json`.

## Context map
| Talks to | How (topic / endpoint / file / import) | Relationship |
|---|---|---|
| Any job client (the Data Migration frame) | sub `APK.audio/System/Protocols/wireless_wwb/outgoing/job`, pub `…/incoming/result` (`schema: 1`) | Published Language (job envelope, versioned) |
| APK.plugin.SDK (`apk-plugin-runner`, `apkaudio-contracts`) | Cargo `tag = "sdk-v0.5"`, patched to the checkout locally and in the Dockerfile; job loop `apk_plugin_runner::jobs::JobWorker` | Conformist, by version |
| DATATYPE.wireless.CSV `SRC/kernel` (`apkaudio-wireless-kernel`) | Cargo git rev (band rule, status word, job envelope) | Shared kernel, one-way |
| MANAGEMENT.DataMigration | depends on `SRC/importer` (`apkaudio-wireless-wwb`) by git rev | Supplier (crate) |
| `apk_wireless_import` (DATATYPE.wireless.CSV/SRC/python) | `IMPORT:file:SHOW/Entry.py` imports it; pinned by rev in `IMPORT:file:SHOW/requirements.txt` | Supplier (package) |
| oaRustCore (optional native wheel) | the Python edge's engine; absent → `STATUS = "unavailable"` | Customer of a native module |
| POD.workers aggregator | `include:`s `Docker/docker-compose.yml`; project `workers` | Pod aggregator (house pattern) |

## Scorecard
| Category | Score (0–3) | Finding |
|---|---|---|
| Coupling & Cohesion | 2 | SDK by tag, parser a crate with one owner, no `#[path]` from another repo; the health bin still compiles the runner's source file (waiting on the runner) and the image context is the repo root until an SDK tag carries `jobs` |
| Boundary Integrity | 3 | Pure parser; MQTT only through the runner's `JobWorker`; versioned answer with a pinned shape |
| Duplication Control | 3 | Job loop is the runner's, envelope and band rule the kernel's; MARKER_HEADERS/lifecycle shared via apk_wireless_import |
| Testing & Contracts | 3 | `SRC/check.sh`: worker (incl. self test, topics, shape), parser + fixture tests, the Python edge's Tests/; DataMigration's frame-header contract test |
| Change management | 2 | Worker 0.2.0, `schema: 1`; nothing tagged yet (tags are the owner's) |
Score key: 3 meets the standard · 2 minor gaps · 1 significant gaps · 0 absent / violated.

## Findings
1. **Health probe compiled from the runner's source file.** `SRC/Cargo.toml` `[[bin]] apk-plugin-health`. Waiting on DISCOVERY.Engine (handoff filed: `apk_plugin_runner::health_check()`).
2. **Image builds from the monorepo root.** `Docker/Dockerfile` COPYs contracts, the runner and the CSV kernel and patches them in with `--config`, because `sdk-v0.5` predates `apk_plugin_runner::jobs` and the kernel rev is not pushed. Needs owner: publish the next SDK tag and push DATATYPE.wireless.CSV.
3. **Good — no port, hardened container.** `cap_drop: [ALL]`, `read_only`, `no-new-privileges`, non-root uid 10001, runner health probe.
4. **Good — topics from the contract.** `job_and_result_topics_are_this_modules` asserts the job/result topic strings.
5. **Good — pure core, thin shell.** `handle_job` is three lines over the kernel; the parser publishes nothing.
