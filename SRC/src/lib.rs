// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The Shure Wireless Workbench show importer as a worker (LOOSE-workers-convert-rest).
//!
//! ```text
//!   browser ── APK.audio/System/Protocols/wireless_wwb/outgoing/job ──▶ worker
//!   worker  ── APK.audio/System/Protocols/wireless_wwb/incoming/result ─▶ browser
//! ```
//!
//! ONE JOB IN, ONE RESULT OUT, MATCHED BY `job_id`. Neither leaf retains (see
//! `contracts/rust/src/baremetal_io.rs`): a retained job would run again on
//! every reconnect, and a retained result would answer a question the next
//! reader never asked.
//!
//! [`handle_job`] is the whole tool and has no bus in it, so the tests below
//! run it on the fixture in `../test_data/` with no broker anywhere near.
//! [`run_job_agent`] hands it to the runner's job loop
//! (`apk_plugin_runner::jobs::JobWorker`); the envelope — `job_id`, `text` |
//! `content_b64`, the size limit, the versioned answer — is the wireless
//! kernel's (`apkaudio_wireless_kernel::job`), shared with the other three
//! wireless workers.

pub use apkaudio_wireless_wwb as wwb_importer;
pub mod carriers;
pub mod self_test;

use apkaudio_wireless_kernel::job;
use serde_json::Value;

/// The token the bus knows this worker by. The same word as `[wireless_wwb]`.
pub const MODULE: &str = "wireless_wwb";

/// The tool the result came from, for a reader that listens to several.
pub const TOOL: &str = "DATATYPE.wireless.SHUREwirelessWorkbench";

/// The compiled-in config, the same text the runner publishes at `/config`.
pub const CONFIG_INI: &str = include_str!("../config.ini");

/// Used when `config.ini` does not say.
const DEFAULT_MAX_JOB_BYTES: usize = 16 * 1024 * 1024;

/// `max_job_bytes` from `config.ini`, or the default.
pub fn max_job_bytes(config_ini: &str) -> usize {
    job::max_job_bytes(config_ini, MODULE, DEFAULT_MAX_JOB_BYTES)
}

/// Answer one job. Always returns a result document, never panics on input.
pub fn handle_job(payload: &[u8], max_bytes: usize) -> Value {
    match job::read_file_job(payload, TOOL, "upload.shw", "the show", max_bytes) {
        Ok(file) => file.answer(TOOL, wwb_importer::classify_wwb_bytes(&file.bytes, &file.filename)),
        Err(failure) => failure,
    }
}

/// The carriers a Workbench show names (ROAD-3519). One `<channel>` of a
/// `<device>` is one carrier. A show carries no serial (its `<mac_address>`
/// is an address, and empty on offline units), so the id is formed
/// (src/carriers.rs) from the device's inventory `<id>` — Workbench's own GUID
/// for the unit — and the channel number; a device with no `<id>` falls back
/// to `source file | device name | model | band | channel number | channel
/// name`. Either way `#n` numbers repeats in show order.
pub fn carriers_of(answer: &Value) -> Option<(String, Vec<carriers::Carrier>)> {
    if answer["ok"] != true {
        return None;
    }
    let result = &answer["result"];
    let file = text(result, "file_path").to_string();
    let list = result["channels"].as_array()?.iter().filter_map(|r| {
        let number = r["channel_number"].as_u64().map(|n| n.to_string()).unwrap_or_default();
        let unit = match r["device_uid"].as_str().filter(|u| !u.trim().is_empty()) {
            Some(uid) => carriers::Carrier::unit_of(&[uid, &number]),
            None => carriers::Carrier::unit_of(&[&file, text(r, "device_name"), text(r, "model"), text(r, "band"), &number, text(r, "channel_name")]),
        };
        let name = [text(r, "channel_name"), text(r, "device_name")].into_iter().find(|s| !s.trim().is_empty()).unwrap_or("");
        Some(carriers::Carrier {
            unit,
            name: name.into(),
            frequency_mhz: r["frequency_mhz"].as_f64()?,
            channel: number,
            band: text(r, "band").into(),
            model: text(r, "model").into(),
            manufacturer: text(r, "manufacturer").into(),
            device: text(r, "device_name").into(),
            zone: text(r, "zone").into(),
            group: String::new(),
            source_file: file.clone(),
        })
    });
    Some((file.clone(), carriers::number_units(list.collect())))
}

/// A text field of a result row; empty when absent.
fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}

/// The carriers of every imported file, restated as a Device/ tree and
/// sighted for presence (src/carriers.rs). Blocking, beside the job loop.
pub fn run_device_agent(host: &str, port: u16) {
    carriers::run(MODULE, host, port, carriers::restate_s(CONFIG_INI, MODULE, 30));
}

/// The job loop: the runner's. Blocking; the runner starts it on its own
/// thread and exits the process if it ever returns, so Docker restarts it.
pub fn run_job_agent(host: &str, port: u16) {
    let max = max_job_bytes(CONFIG_INI);
    apk_plugin_runner::jobs::JobWorker::new(MODULE, TOOL, move |payload: &[u8]| {
        // One job taken off outgoing/job and run: this worker's packet (ROAD-3472).
        apk_plugin_runner::loopback::packet_received();
        let answer = handle_job(payload, max);
        // Its carriers on the bus as a Device/ tree (ROAD-3519).
        if let Some((file, list)) = carriers_of(&answer) {
            carriers::imported(&file, list);
        }
        answer
    })
        // A job is a file, and base64 grows it by a third.
        .max_packet(max / 3 * 4 + 64 * 1024)
        .describe(|a| format!("ok={} channels={}", a["ok"], a["result"]["total_channels"]))
        .run(host, port);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest the binary compiles in parses, declares the loopback port
    /// and lists every key of config.ini's [wireless_wwb] that code reads (ROAD-3472).
    #[test]
    fn the_compiled_in_manifest_parses_and_reads_every_config_key() {
        let manifest = apkaudio_contracts::embedded_manifest!().expect("agent.toml parses");
        assert_eq!(manifest.name, MODULE);
        assert_eq!(manifest.kind, "worker");
        assert_eq!(manifest.telemetry_port, 8228);
        let config = apk_plugin_runner::ini_to_json(CONFIG_INI);
        let keys: Vec<&str> = config[MODULE].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(manifest.unread_config_keys(keys), Some(0), "a [wireless_wwb] key nothing reads");
    }
    use apkaudio_contracts::baremetal_io;
    use base64::Engine as _;
    use serde_json::json;

    const REPORT: &str = include_str!("../../test_data/venue.shw");

    fn job(body: Value) -> Vec<u8> {
        body.to_string().into_bytes()
    }

    #[test]
    fn a_text_job_is_parsed_and_answered_under_its_id() {
        let out = handle_job(&job(json!({"job_id": "j1", "filename": "venue.shw", "text": REPORT})), 1 << 24);
        assert_eq!(out["job_id"], "j1");
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(out["result"]["total_channels"], 186);
        assert_eq!(out["result"]["status"], "success");
        assert_eq!(out["result"]["file_path"], "venue.shw");
        assert_eq!(out["result"]["channels"][0]["model"], "PSM1000");
    }

    #[test]
    fn a_base64_job_is_the_same_as_its_text() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(REPORT);
        let a = handle_job(&job(json!({"job_id": "j2", "filename": "venue.shw", "content_b64": b64})), 1 << 24);
        let b = handle_job(&job(json!({"job_id": "j2", "filename": "venue.shw", "text": REPORT})), 1 << 24);
        assert_eq!(a["ok"], true, "{a}");
        assert_eq!(a["result"], b["result"]);
    }

    /// The case `content_b64` exists for: a zipped show, which no text field
    /// could carry intact.
    #[test]
    fn a_zipped_show_as_base64_reads_the_same_as_the_plain_one() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zw.start_file("venue.shw", opts).unwrap();
            zw.write_all(REPORT.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        let out = handle_job(&job(json!({"job_id": "z", "filename": "venue.shw", "content_b64": b64})), 1 << 24);
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(out["result"]["total_channels"], 186);
    }

    #[test]
    fn a_bad_job_is_answered_not_dropped() {
        let out = handle_job(b"not json", 1 << 24);
        assert_eq!(out["ok"], false);
        assert!(out["job_id"].is_null());

        let out = handle_job(&job(json!({"text": REPORT})), 1 << 24);
        assert_eq!(out["ok"], false, "no job_id");

        let out = handle_job(&job(json!({"job_id": "j3"})), 1 << 24);
        assert_eq!(out["job_id"], "j3");
        assert_eq!(out["ok"], false, "no content");

        let out = handle_job(&job(json!({"job_id": "j4", "content_b64": "%%%"})), 1 << 24);
        assert_eq!(out["ok"], false, "bad base64");
    }

    #[test]
    fn an_oversized_job_is_refused_by_size() {
        let out = handle_job(&job(json!({"job_id": "j5", "text": REPORT})), 100);
        assert_eq!(out["ok"], false);
        assert!(out["error"].as_str().unwrap().contains("at most 100"));
    }

    #[test]
    fn the_limit_comes_from_config_ini() {
        assert_eq!(max_job_bytes(CONFIG_INI), 16_777_216);
        assert_eq!(max_job_bytes("[wireless_wwb]\n"), DEFAULT_MAX_JOB_BYTES);
    }

    #[test]
    fn job_and_result_topics_are_this_modules() {
        let (job, retain) = baremetal_io::module_job(MODULE).unwrap();
        assert_eq!(job, "APK.audio/System/Protocols/wireless_wwb/outgoing/job");
        assert!(!retain);
        let (result, retain) = baremetal_io::module_result(MODULE).unwrap();
        assert_eq!(result, "APK.audio/System/Protocols/wireless_wwb/incoming/result");
        assert!(!retain);
    }

    /// The wire shape a browser reads. A parser field renamed is a failure
    /// here first; change it on purpose, together with `job::SCHEMA`.
    #[test]
    fn the_result_shape_is_pinned() {
        let out = handle_job(&job(json!({"job_id": "s", "filename": "venue.shw", "content_b64": base64::engine::general_purpose::STANDARD.encode(REPORT)})), 1 << 24);
        assert_eq!(out["ok"], true, "{out}");
        assert_eq!(out["schema"], job::SCHEMA);
        let keys = |v: &Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(keys(&out["result"]), ["channels", "file_path", "skipped_rows", "source_format", "status", "total_channels"]);
        assert_eq!(keys(&out["result"]["channels"][0]), ["band", "channel_name", "channel_number", "device_name", "device_uid", "frequency_mhz", "manufacturer", "model", "zone"]);
        assert_eq!(job::SCHEMA, 1);
    }
    /// ROAD-3519: every carrier of the fixture becomes a Device/ row with
    /// PLUGIN.WIFI's leaf names, under an id formed from the file, not an address.
    #[test]
    fn the_fixture_puts_every_carrier_under_device() {
        let out = handle_job(&job(json!({"job_id": "d", "filename": "venue.shw", "text": REPORT})), 1 << 24);
        assert_eq!(out["ok"], true, "{out}");
        let (file, list) = carriers_of(&out).expect("an ok answer has carriers");
        assert_eq!(file, "venue.shw");
        assert_eq!(list.len() as u64, out["result"]["total_channels"].as_u64().unwrap());
        assert!(!list.is_empty());
        let ids: std::collections::BTreeSet<String> = list.iter().map(|c| c.device_id(MODULE)).collect();
        assert_eq!(ids.len(), list.len(), "every carrier its own id");
        let mut bands = 0;
        for c in &list {
            let f = c.fields(MODULE);
            let get = |leaf: &str| f.iter().find(|(t, _)| t.ends_with(&format!("/{leaf}"))).map(|(_, v)| v.clone()).unwrap();
            assert!(f[0].0.starts_with("APK.audio/System/Protocols/wireless_wwb/Device/wireless_wwb_plan_"), "{}", f[0].0);
            assert_eq!(get("device_class"), "Wireless Transmitter");
            let mhz: f64 = get("frequency_mhz").parse().unwrap();
            assert!(mhz > 100.0 && mhz < 3000.0, "{mhz}");
            assert!(!get("channel").is_empty() && !get("name").is_empty());
            if get("band") != "-" {
                bands += 1;
            }
            assert!(c.device_id(MODULE).starts_with("wireless_wwb:plan:"));
        }
        assert!(bands > 0, "the band leaf is said (the file's, else the ITU band)");
        // The same file again lands on the same ids.
        let (_, again) = carriers_of(&handle_job(&job(json!({"job_id": "d", "filename": "venue.shw", "text": REPORT})), 1 << 24)).unwrap();
        assert_eq!(again.iter().map(|c| c.device_id(MODULE)).collect::<Vec<_>>(), list.iter().map(|c| c.device_id(MODULE)).collect::<Vec<_>>());
        assert!(carriers_of(&handle_job(b"not json", 1 << 24)).is_none(), "a failed job names no carrier");
    }
}
