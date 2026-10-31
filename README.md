# DATATYPE.wireless.SHUREwirelessWorkbench

> 📑 **Shure Wireless Workbench importer** — reads a WWB show file (`.shw`, plain XML or zipped) and publishes its wireless channels, one record per channel, on the APK.audio bus.
>
> 📚 **Wiki:** [Documentation/README.md](Documentation/README.md) — principles, architecture, delivery.

---

## 🧩 Architectural Role
* **Family**: DATATYPE — data-format importers and parsers (`PODS/POD.THIN/Softapps/DATATYPE/`)
* **Category**: wireless coordination — source 3 of the four Data Migration importers (IAS, Soundbase, WWB, CSV)
* **Pipeline Stage**: DATA — a report in someone else's format becomes the channel record the Data Migration tab renders

---

## ⚙️ Repository Overview

The parser is source compiled by somebody else; `SRC/` is the worker crate that runs it on its own:

| File | What it is | Built / loaded by |
| :-- | :-- | :-- |
| `wwb_importer.rs` | The parser (no side effect; it does not publish) | `#[path]` module `wwb_importer` of the `apkaudio-data-migration` crate (`Softapps/MANAGEMENT_AND_UTILITIES/MANAGEMENT.DataMigration/mod.rs`), which `oaRustCore` re-exports as `data_migration` |
| `test_wwb_importer.rs` | A standalone runner over the fixture | `[[bin]] test_wwb_importer` in that crate's `Cargo.toml` |
| `IMPORT:file:SHOW/Entry.py` | The Python edge: hands a show file to `oaRustCore.showfile_unpacker` | loaded by file path; its `STATUS` is asserted by `NODE.BareMetal/SRC/Core/TEST:runner:PYTHON/test_handler_status.py` |
| `test_data/venue.shw` | A real WWB 7.4 show file (plain XML, 100 devices, 186 channels) | the runner |
| `display.json` | Launcher manifest (category `DATATYPE`) | the OS app tree (`OS/system/project-apps.json`) |
| `SRC/` | The worker crate `apk-worker-wireless-wwb`: `wwb_importer.rs` (by `#[path]`) behind a job loop on `apk-plugin-runner`, with its `config.ini` (section `[wireless_wwb]`) | `cargo test --manifest-path SRC/Cargo.toml`; the image `worker-wireless-wwb:local` |
| `Docker/` | The worker stack: container `Worker-WIRELESS_WWB`, project `workers`, no port | included by `PODS/POD.workers/Docker/docker-compose.yml` |

### What the parser does (`wwb_importer.rs`)

- `parse_wwb_bytes(bytes, name)` is the door for anything that may be a zip: bytes starting with the zip magic `PK\x03\x04` are unpacked and every member parsed; anything else is read as XML text. A zip that will not open is an error, not an empty show. `parse_wwb_file(path)` reads the file and calls it; `parse_wwb_str(text, name)` parses XML text already in hand. The `classify_wwb_bytes` / `classify_wwb_str` halves do the same without publishing (and are what the unit tests call).
- The XML is read **by its markup** (a small scanner in `wwb_importer.rs`: tags, CDATA, entities), so a minified show reads like an indented one: a `<device>`'s own children give `manufacturer`, `model`, `device_name`, `band` and `zone`; each `<channel number="n">` gives `channel_name` and `frequency` in kHz.
- A channel is kept only when its frequency converts to between 100 and 3000 MHz. Nothing is invented: a device that names no manufacturer or model, or a channel with no `number=`, gives `null` for that field (a channel outside any `<device>` has no device fields).
- The result (`WwbParseResult`: `status`, `source_format` = `Shure_WWB_SHW`, `file_path`, `mqtt_topic`, `total_channels`, `channels[]`) is returned, not published: the `wireless_wwb` worker (`SRC/src/lib.rs`) wraps it as `{job_id, ok, result, error}` and answers, not retained, on `APK.audio/System/Protocols/wireless_wwb/incoming/result`. (`mqtt_topic` still names `…/DataMigration/WWB/ImportedData` from `wwb_mqtt_topic()`; nothing publishes there since the orchestrator's DataMigration inbox was retired.)
- Each channel record: `device_name`, `manufacturer`, `model`, `band`, `channel_number`, `channel_name`, `frequency_mhz`, `zone` — the eight headers the GUI frame `WEB.Frontend-Assets/SRC/FrontEnd/Gui_Frames/3_Data_Migration/3_Wireless_Workbench/Wireless_Workbench_Importer.json` declares for its table.

---

### As a worker (`Worker-WIRELESS_WWB`, POD.workers)

The same parser answers jobs on the bus (LOOSE-workers-convert-rest). A client subscribes to
`APK.audio/System/Protocols/wireless_wwb/incoming/result`, then publishes
`{"job_id": "<yours>", "filename": "venue.shw", ...}` to `APK.audio/System/Protocols/wireless_wwb/outgoing/job`
with the file as "content_b64" (the file's bytes; a zipped show does not survive as text — plain XML may come as "text"). The answer carries the same `job_id`, `ok`, and the parse result of
`classify_wwb_bytes` under `result`; a malformed job or one over `max_job_bytes` (16 MiB, `SRC/config.ini`) comes
back `ok: false` with an `error`. Nothing is retained and the worker opens no port. Measured
2026-10-30: the fixture `test_data/venue.shw` sent as a job came back `ok: true` with 186 channels.

```bash
docker compose -f PODS/POD.workers/Docker/docker-compose.yml up -d --build wireless-wwb
cargo test --manifest-path SRC/Cargo.toml        # handle_job on the fixture, the self test
```

## 🚀 Usage & Integration
* **Web Portal Entry**: Data Migration → *Wireless Workbench* (GUI frame `Gui_Frames/3_Data_Migration/3_Wireless_Workbench/`). The frame posts the chosen file's bytes, base64, as a job to `APK.audio/System/Protocols/wireless_wwb/outgoing/job` and draws the table from `.../wireless_wwb/incoming/result`.
* **Who calls the parser**: the `wireless_wwb` worker, on each job the frame posts: `content_b64` (or `text`) becomes **bytes** for `classify_wwb_bytes`, so a zipped `.shw` is unzipped rather than lossy-decoded to text. (The orchestrator's DataMigration inbox that used to call it is retired: LOOSE-datamigration-frames-to-workers.)
* **Remote Repository**: https://github.com/LikeDotAudio/APK.softapp.DATATYPE.wireless.SHUREwirelessWorkbench

### Build and check

```bash
cd PODS/POD.THIN/Softapps/MANAGEMENT_AND_UTILITIES/MANAGEMENT.DataMigration
cargo test --lib wwb               # plain, zipped, broken-zip, minified .shw; nothing invented; publishes nothing
cargo check --bins --lib          # compiles this parser and test_wwb_importer
cd PODS/POD.THICK/NODE.BareMetal/SRC/Core
PYTHONPATH=. python3 TEST:runner:PYTHON/test_handler_status.py   # Entry.py's STATUS — 6 tests
```

`cargo run --bin test_wwb_importer` parses the fixture and prints the first 15 channels; it publishes nothing. It reads the fixture from `../../DATATYPE/DATATYPE.wireless.SHUREwirelessWorkbench/test_data/venue.shw` (until 2026-10-30 it looked under the DataMigration crate and exited 1).

`Entry.py`'s `run_tests()` unpacks a one-member zip through the Rust unpacker and checks the answer; it returns `False` when the unpacker is missing or wrong (it used to return `True` unconditionally). `start()`/`stop()` are declared no-ops — the importer holds nothing between calls.

---

## 🧭 Philosophy

- **One record out, whatever came in.** Four foreign formats feed the Data Migration tab; each importer emits the fields its GUI frame declares as headers (the rule in the `MANAGEMENT.DataMigration/mod.rs` header; the `check.sh datamigration` gate it names is not in the tree today). Here: `WwbChannel` in `wwb_importer.rs` against `Wireless_Workbench_Importer.json`.
- **Topics come from the contract.** `wwb_mqtt_topic()` builds the topic with `apkaudio_contracts::topics::data_migration` rather than a literal, so a rename of the root is one edit.
- **A stub says so in a constant.** `IMPORT:file:SHOW/Entry.py` sets `STATUS = "live"` only when `oaRustCore.showfile_unpacker` imports, `"unavailable"` otherwise, and a test asserts it. When the Rust unpacker is missing it raises `RuntimeError("Rust Showfile parser required")` instead of unpacking in Python — one implementation, no quiet fallback.
- **Publish and let go.** The daemon publishes the parse retained and returns; the bus holds the latest import for any tab that opens later. There is no store of imports in this repo.

---

## 🗺 Plans

None in this repo. Plans live in `Documentation/🛣Roadmap/roadmap.htm` of the APK.audio repo.

<!-- wiki:smoke -->
**Smoke test** — ✅ PASS 1/1 since 2026-10-30 12:51 EDT — [results](Documentation/%F0%9F%A7%A9Manual%20and%20Artifacts/SMOKE.md)
<!-- /wiki:smoke -->
