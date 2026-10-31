# Architecture — DATATYPE.wireless.SHUREwirelessWorkbench

← [Wiki home](../README.md)

## Using it

In the Data Migration tab, the *Wireless Workbench* frame (`Gui_Frames/3_Data_Migration/3_Wireless_Workbench/`) reads the chosen `.shw` as bytes in the browser and posts it as a job, `{job_id, filename, content_b64}` (plain or zipped), not retained, to `APK.audio/System/Protocols/wireless_wwb/outgoing/job`; it draws its table from the worker's answer on `…/wireless_wwb/incoming/result`. No host path is ever sent.

## How it works

```text
…/wireless_wwb/outgoing/job ─▶ Worker-WIRELESS_WWB (SRC/src/lib.rs handle_job)
                    content_b64 → bytes ┐
                    text        → bytes ┴─▶ classify_wwb_bytes (no side effect) ─▶ {job_id, ok, result, error},
                                               not retained → …/wireless_wwb/incoming/result
classify_wwb_bytes: starts with PK\x03\x04 → unzip, parse every member as XML; else → text
parse_wwb_xml_content: xml_events (tags, CDATA, entities) + an element stack — line breaks do not matter
   <device> … </device>      its own children: manufacturer, model (null when absent), device_name, band, zone
   <channel number="n">      number (null when absent), channel_name, frequency (kHz) → MHz; keep 100–3000 MHz
```

`WwbChannel { device_name, manufacturer: Option, model: Option, band, channel_number: Option, channel_name, frequency_mhz, zone }`.

## Files

| Path | What |
|---|---|
| `SRC/importer/` | the parser crate `apkaudio-wireless-wwb` and its unit tests (no publisher), `tests/fixtures.rs` over `test_data/venue.shw`; MANAGEMENT.DataMigration depends on it by git rev |
| `SRC/check.sh` | the one local check: `cargo test --workspace --locked` (+ pytest for the Python edge) |
| `test_data/venue.shw` | a real WWB 7.4 show (plain XML) |
| `IMPORT.file.SHOW/Entry.py` | Python edge over `oaRustCore.showfile_unpacker.unpack_showfile` (returns each `.json` member as a string) |

## Testing

```bash
bash PODS/POD.THIN/Softapps/DATATYPE/DATATYPE.wireless.SHUREwirelessWorkbench/SRC/check.sh
```

## As a worker

`SRC/src/lib.rs` `handle_job` decodes a job (`text` or `content_b64`), refuses one over
`max_job_bytes` (16 MiB), runs `classify_wwb_bytes` and returns `{schema, job_id, tool, ok, result, error, elapsed_ms}` (the wireless kernel's `job` envelope)
— always, so a caller waiting on a `job_id` is never left waiting. `run_job_agent` (the runner's `JobWorker`: session, ConnAck resubscribe, lane thread, busy answer) feeds it from
`APK.audio/System/Protocols/wireless_wwb/outgoing/job` and answers on `…/incoming/result`, neither
retained. `apk-plugin-runner` adds the retained `incoming/api` description, the config, the will and
the self test (`SRC/src/self_test.rs`: a probe file compiled into the binary runs through
`handle_job`). The pod is `PODS/POD.workers`.
