# Architecture — DATATYPE.wireless.SHUREwirelessWorkbench

← [Wiki home](../README.md)

## Using it

In the Data Migration tab, the *Wireless Workbench* frame (`Gui_Frames/3_Data_Migration/3_Wireless_Workbench/`) publishes the chosen `.shw` on `APK.audio/System/DataMigration/WWB/FilePath` — a disk path, `raw_content` text, or a base64 `raw_blob` (plain or zipped) — and draws its table from `…/WWB/ImportedData`.

## How it works

```text
…/WWB/FilePath ─▶ NODE.BareMetal data-migration daemon
                    raw_blob    → parse_wwb_bytes(bytes)  ┐
                    raw_content → parse_wwb_str(text)     ├─▶ classify_* (no side effect) ─▶ publish retained
                    a path      → parse_wwb_file(path)    ┘      → APK.audio/System/DataMigration/WWB/ImportedData
classify_wwb_bytes: starts with PK\x03\x04 → unzip, parse every member as XML; else → text
parse_wwb_xml_content: xml_events (tags, CDATA, entities) + an element stack — line breaks do not matter
   <device> … </device>      its own children: manufacturer, model (null when absent), device_name, band, zone
   <channel number="n">      number (null when absent), channel_name, frequency (kHz) → MHz; keep 100–3000 MHz
```

`WwbChannel { device_name, manufacturer: Option, model: Option, band, channel_number: Option, channel_name, frequency_mhz, zone }`.

## Files

| Path | What |
|---|---|
| `wwb_importer.rs` | parser and unit tests (it does not publish); module `wwb_importer` of `MANAGEMENT.DataMigration` |
| `test_wwb_importer.rs` | `[[bin]] test_wwb_importer` — parses `test_data/venue.shw` and prints the first 15 channels |
| `test_data/venue.shw` | a real WWB 7.4 show (plain XML) |
| `IMPORT:file:SHOW/Entry.py` | Python edge over `oaRustCore.showfile_unpacker.unpack_showfile` (returns each `.json` member as a string) |

## Testing

```bash
cd PODS/POD.THIN/Softapps/MANAGEMENT_AND_UTILITIES/MANAGEMENT.DataMigration
cargo test --lib wwb        # plain, zipped, broken zip, minified, nothing invented
cd PODS/POD.THICK/NODE.BareMetal/SRC/Core
PYTHONPATH=. python3 -m pytest -q "TEST:runner:PYTHON/test_handler_status.py"
```

## As a worker

`SRC/src/lib.rs` `handle_job` decodes a job (`text` or `content_b64`), refuses one over
`max_job_bytes` (16 MiB), runs `classify_wwb_bytes` and returns `{job_id, tool, ok, result, error, elapsed_ms}`
— always, so a caller waiting on a `job_id` is never left waiting. `run_job_agent` feeds it from
`APK.audio/System/Protocols/wireless_wwb/outgoing/job` and answers on `…/incoming/result`, neither
retained. `apk-plugin-runner` adds the retained `incoming/api` description, the config, the will and
the self test (`SRC/src/self_test.rs`: a probe file compiled into the binary runs through
`handle_job`). The pod is `PODS/POD.workers`.
