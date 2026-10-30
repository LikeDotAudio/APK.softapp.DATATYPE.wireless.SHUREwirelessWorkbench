# Principles — DATATYPE.wireless.SHUREwirelessWorkbench

The rules the Wireless Workbench importer keeps, each with the code that keeps it (in `wwb_importer.rs` unless named). Ecosystem rules are cited, not copied: [Refusal over fabrication](../../../../../../../Documentation/%F0%9F%8F%9BPhilosophy/Doctrine/readme.Refusal%20over%20fabrication.md).

← [Wiki home](../README.md)

| Principle | Kept by | Where it breaks |
|---|---|---|
| **Bytes until proven text.** A zipped `.shw` is recognised by its magic bytes and unpacked; text decoding happens only for something that is not a zip. | `ZIP_MAGIC` (line 42), `classify_wwb_bytes` (line 71); NODE.BareMetal's daemon hands a `raw_blob` over as bytes (`parse_wwb_bytes`, line 53) | — (until 2026-10-30 a zipped upload was lossy-decoded and parsed as XML) |
| **A broken input is an error, not an empty show.** | `classify_wwb_bytes` returns `Err` for an unreadable zip; test `a_broken_zip_is_an_error_not_an_empty_show` (line 292) | A plain-XML file that holds no recognisable `<channel>` is still "success" with 0 channels |
| **Parsing has no side effect; publishing is a separate step.** | `classify_wwb_*` (lines 71, 91) vs `parse_wwb_*` (lines 45–62) | — |
| **One record out, whatever came in.** `WwbChannel` carries the eight headers the GUI frame declares. | `WwbChannel`; `Wireless_Workbench_Importer.json` | — |
| **Topics come from the contract.** | `wwb_mqtt_topic()` (line 36) | — |
| **Do not invent a value.** | — | A device that names no manufacturer or model is recorded as `Shure` / `Generic` (lines 115–116, 131–132); a channel without `number=` gets the previous number or 1 |
| **A stub says so in a constant; a self-test tests.** | `IMPORT:file:SHOW/Entry.py`: `STATUS` (line 33); `run_tests()` (line 65) unpacks a one-member zip and checks the answer; `start`/`stop` are declared stateless no-ops | — |
| **The XML is read line by line, not parsed.** | `for line in content.lines()` (line 127) | A show file whose tags are not one per line (minified XML) yields nothing |
