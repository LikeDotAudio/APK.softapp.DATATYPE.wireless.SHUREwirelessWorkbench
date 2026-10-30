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
parse_wwb_xml_content: line by line
   <device> … </device>      manufacturer, model, device_name (CDATA), band, zone
   <channel number="n">      channel_name, frequency (kHz) → MHz; keep 100–3000 MHz
```

`WwbChannel { device_name, manufacturer, model, band, channel_number, channel_name, frequency_mhz, zone }`.

## Files

| Path | What |
|---|---|
| `wwb_importer.rs` | parser, publisher and unit tests; module `wwb_importer` of `MANAGEMENT.DataMigration` |
| `test_wwb_importer.rs` | `[[bin]] test_wwb_importer` — **publishes retained** |
| `test_data/venue.shw` | a real WWB 7.4 show (plain XML) |
| `IMPORT:file:SHOW/Entry.py` | Python edge over `oaRustCore.showfile_unpacker.unpack_showfile` (returns each `.json` member as a string) |

## Testing

```bash
cd PODS/POD.THIN/Softapps/MANAGEMENT_AND_UTILITIES/MANAGEMENT.DataMigration
cargo test --lib wwb        # plain, zipped, broken zip
cd PODS/POD.THICK/NODE.BareMetal/SRC/Core
PYTHONPATH=. python3 -m pytest -q "TEST:runner:PYTHON/test_handler_status.py"
```
