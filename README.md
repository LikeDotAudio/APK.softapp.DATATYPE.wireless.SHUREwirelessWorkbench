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

This repo holds source files, not a crate. They are compiled by somebody else:

| File | What it is | Built / loaded by |
| :-- | :-- | :-- |
| `wwb_importer.rs` | The parser and its MQTT publisher | `#[path]` module `wwb_importer` of the `apkaudio-data-migration` crate (`Softapps/MANAGEMENT_AND_UTILITIES/MANAGEMENT.DataMigration/mod.rs`), which `oaRustCore` re-exports as `data_migration` |
| `test_wwb_importer.rs` | A standalone runner over the fixture | `[[bin]] test_wwb_importer` in that crate's `Cargo.toml` |
| `IMPORT:file:SHOW/Entry.py` | The Python edge: hands a show file to `oaRustCore.showfile_unpacker` | loaded by file path; its `STATUS` is asserted by `NODE.BareMetal/SRC/Core/TEST:runner:PYTHON/test_handler_status.py` |
| `test_data/venue.shw` | A real WWB 7.4 show file (plain XML, 100 devices, 186 channels) | the runner |
| `display.json` | Launcher manifest (category `DATATYPE`) | the OS app tree (`OS/system/project-apps.json`) |

### What the parser does (`wwb_importer.rs`)

- `parse_wwb_bytes(bytes, name)` is the door for anything that may be a zip: bytes starting with the zip magic `PK\x03\x04` are unpacked and every member parsed; anything else is read as XML text. A zip that will not open is an error, not an empty show. `parse_wwb_file(path)` reads the file and calls it; `parse_wwb_str(text, name)` parses XML text already in hand. The `classify_wwb_bytes` / `classify_wwb_str` halves do the same without publishing (and are what the unit tests call).
- The XML is read **line by line**, not with an XML parser: `<device>` … `</device>` blocks give `manufacturer`, `model`, `device_name` (CDATA), `band` and `zone`; each `<channel number="n">` gives `channel_name` and `frequency` in kHz.
- A channel is kept only when its frequency converts to between 100 and 3000 MHz. Manufacturer defaults to `Shure` and model to `Generic` when a device does not name them.
- The result (`WwbParseResult`: `status`, `source_format` = `Shure_WWB_SHW`, `file_path`, `mqtt_topic`, `total_channels`, `channels[]`) is published **retained, QoS 1** to `APK.audio/System/DataMigration/WWB/ImportedData` — the topic is built by `apkaudio_contracts::topics::data_migration("WWB", "ImportedData")`, not typed out. It connects to the loopback broker (`MQTT_LOOPBACK_HOST`, `MQTT_DEFAULT_PORT`) with the broker credentials from `apkaudio_contracts::broker_auth`, waits up to 5 s for the ack, and returns the result either way.
- Each channel record: `device_name`, `manufacturer`, `model`, `band`, `channel_number`, `channel_name`, `frequency_mhz`, `zone` — the eight headers the GUI frame `WEB.Frontend-Assets/SRC/FrontEnd/Gui_Frames/3_Data_Migration/3_Wireless_Workbench/Wireless_Workbench_Importer.json` declares for its table.

---

## 🚀 Usage & Integration
* **Web Portal Entry**: Data Migration → *Wireless Workbench* (GUI frame `Gui_Frames/3_Data_Migration/3_Wireless_Workbench/`). The frame publishes the chosen file on `APK.audio/System/DataMigration/WWB/FilePath` and draws the table from `.../WWB/ImportedData`.
* **Who calls the parser**: the Data Migration daemon in the orchestrator (`NODE.BareMetal/SRC/Core/ORCHESTRATOR:hunter:VISA/src/services/data_migration.rs`, started from `main.rs`) subscribes `APK.audio/System/DataMigration/+/FilePath`. On a `/WWB/` topic a base64 `raw_blob` is decoded and its **bytes** go to `parse_wwb_bytes` (so a zipped `.shw` upload is unzipped — it used to be lossy-decoded to text and parsed as XML, yielding nothing); a `raw_content` string goes to `parse_wwb_str`; anything else is taken as a path on disk for `parse_wwb_file`.
* **Remote Repository**: https://github.com/LikeDotAudio/APK.softapp.DATATYPE.wireless.SHUREwirelessWorkbench

### Build and check

```bash
cd PODS/POD.THIN/Softapps/MANAGEMENT_AND_UTILITIES/MANAGEMENT.DataMigration
cargo test --lib wwb               # plain, zipped and broken-zip .shw; publishes nothing
cargo check --bins --lib          # compiles this parser and test_wwb_importer
cd PODS/POD.THICK/NODE.BareMetal/SRC/Core
PYTHONPATH=. python3 TEST:runner:PYTHON/test_handler_status.py   # Entry.py's STATUS — 6 tests
```

`cargo run --bin test_wwb_importer` parses the fixture **and publishes the result, retained, to the local broker** — it is not a dry run. It reads the fixture from `../../DATATYPE/DATATYPE.wireless.SHUREwirelessWorkbench/test_data/venue.shw` (until 2026-10-30 it looked under the DataMigration crate and exited 1).

`Entry.py`'s `run_tests()` unpacks a one-member zip through the Rust unpacker and checks the answer; it returns `False` when the unpacker is missing or wrong (it used to return `True` unconditionally). `start()`/`stop()` are declared no-ops — the importer holds nothing between calls.

---

## 🧭 Philosophy

- **One record out, whatever came in.** Four foreign formats feed the Data Migration tab; each importer emits the fields its GUI frame declares as headers (the rule in the `MANAGEMENT.DataMigration/mod.rs` header; the `check.sh datamigration` gate it names is not in the tree today). Here: `WwbChannel` in `wwb_importer.rs` against `Wireless_Workbench_Importer.json`.
- **Topics come from the contract.** `wwb_mqtt_topic()` builds the topic with `apkaudio_contracts::topics::data_migration` rather than a literal, so a rename of the root is one edit.
- **A stub says so in a constant.** `IMPORT:file:SHOW/Entry.py` sets `STATUS = "live"` only when `oaRustCore.showfile_unpacker` imports, `"unavailable"` otherwise, and a test asserts it. When the Rust unpacker is missing it raises `RuntimeError("Rust Showfile parser required")` instead of unpacking in Python — one implementation, no quiet fallback.
- **Publish and let go.** The parser publishes retained and returns; the bus holds the latest import for any tab that opens later. There is no store of imports in this repo.

---

## 🗺 Plans

None in this repo. Plans live in `Documentation/🛣Roadmap/roadmap.htm` of the APK.audio repo.

<!-- wiki:smoke -->
**Smoke test** — ✅ PASS 1/1 since 2026-10-30 12:51 EDT — [results](Documentation/%F0%9F%A7%A9Manual%20and%20Artifacts/SMOKE.md)
<!-- /wiki:smoke -->
