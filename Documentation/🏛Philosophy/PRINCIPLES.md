# Principles — DATATYPE.wireless.SHUREwirelessWorkbench

The rules the Wireless Workbench importer keeps, each with the code that keeps it (in `SRC/importer/src/lib.rs` unless named). Ecosystem rules are cited, not copied: [Refusal over fabrication](../../../../../../../Documentation/%F0%9F%8F%9BPhilosophy/Doctrine/readme.Refusal%20over%20fabrication.md).

← [Wiki home](../README.md)

| Principle | Kept by | Where it breaks |
|---|---|---|
| **Bytes until proven text.** A zipped `.shw` is recognised by its magic bytes and unpacked; text decoding happens only for something that is not a zip. | `ZIP_MAGIC`, `classify_wwb_bytes`; NODE.BareMetal's daemon hands a `raw_blob` over as bytes (`parse_wwb_bytes`, line 53) | — (until 2026-10-30 a zipped upload was lossy-decoded and parsed as XML) |
| **A broken input is an error, not an empty show.** | `classify_wwb_bytes` returns `Err` for an unreadable zip; test `a_broken_zip_is_an_error_not_an_empty_show` | A plain-XML file that holds no recognisable `<channel>` is still "success" with 0 channels |
| **Parsing has no side effect; publishing is a separate step.** | `classify_wwb_*` (lines 71, 91) vs `parse_wwb_*` (lines 45–62) | — |
| **One record out, whatever came in.** `WwbChannel` carries the eight headers the GUI frame declares. | `WwbChannel`; `Wireless_Workbench_Importer.json` | — |
| **Do not invent a value.** | `WwbChannel.manufacturer`, `.model` and `.channel_number` are `Option`s: what the show leaves out is `null`, not `Shure` / `Generic` / the previous number; a channel outside a `<device>` inherits nothing; test `what_the_show_leaves_out_stays_out` | — |
| **A stub says so in a constant; a self-test tests.** | `IMPORT.file.SHOW/Entry.py`: `STATUS`; `run_tests()` unpacks a one-member zip and checks the answer; `start`/`stop` are declared stateless no-ops | — |
| **Read the markup, not the line breaks.** | `xml_events` scans tags, CDATA and entities (no new dependency) and `parse_wwb_xml_content` walks an element stack; test `a_minified_show_reads_the_same_as_the_indented_one` | Not a validating parser: malformed XML is read as far as it goes |
| **One parser, one owner.** The parser is the crate `apkaudio-wireless-wwb`; the worker links it by path, MANAGEMENT.DataMigration by git rev. | `SRC/importer/Cargo.toml`; `SRC/Cargo.toml` | — (until 2026-10-30 DataMigration compiled this file by `#[path]` and built its runner from here) |
| **What the four wireless importers share is written once.** The band rule, the status word and the job envelope are the wireless kernel's; the job loop is the runner's. | `apkaudio_wireless_kernel::{usable_mhz…, import_status, job}`; `apk_plugin_runner::jobs::JobWorker` (`SRC/src/lib.rs`) | — (until 2026-10-30 the loop and envelope were copied line for line from the CSV worker) |
| **The answer is versioned and its shape pinned.** | `schema` (`job::SCHEMA`); test `the_result_shape_is_pinned` | the result is the parser's struct serialised, held by the shape test rather than a second DTO type |
| **Builds from its own repository.** Runner and contracts by `tag = "sdk-v0.7"`, the wireless kernel by its pushed rev; Cargo.lock pins those git sources; the image builds from `SRC/` alone. | `SRC/Cargo.toml`, `SRC/Cargo.lock`; `Docker/Dockerfile` (context `../SRC`) | building against a local runner checkout needs a hand-added, git-excluded `[patch]` |
