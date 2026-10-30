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
//! run it on `../test_data/venue.shw` with no broker anywhere near.
//! [`run_job_agent`] is the thin loop that feeds it from MQTT. The shape is the
//! wireless CSV worker's (`DATATYPE.wireless.CSV/SRC/src/lib.rs`).

#[path = "../../wwb_importer.rs"]
pub mod wwb_importer;
pub mod self_test;

use std::time::{Duration, Instant};

use apkaudio_contracts::baremetal_io;
use base64::Engine as _;
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use serde_json::{Value, json};

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
    apk_plugin_runner::ini_to_json(config_ini)
        .get(MODULE)
        .and_then(|s| s.get("max_job_bytes"))
        .and_then(Value::as_str)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_MAX_JOB_BYTES)
}

/// Answer one job. Always returns a result document, never panics on input:
/// a malformed job gets `ok: false` and an `error`, with its `job_id` when it
/// had one, so the caller waiting on that id is not left waiting forever.
pub fn handle_job(payload: &[u8], max_bytes: usize) -> Value {
    let started = Instant::now();
    let job: Value = match serde_json::from_slice(payload) {
        Ok(v @ Value::Object(_)) => v,
        Ok(_) => return failure(Value::Null, "a job is a JSON object", started),
        Err(e) => return failure(Value::Null, &format!("the job is not JSON: {e}"), started),
    };
    let job_id = match job.get("job_id") {
        Some(Value::String(s)) if !s.is_empty() => Value::String(s.clone()),
        _ => return failure(Value::Null, "the job has no job_id", started),
    };
    let filename = job.get("filename").and_then(Value::as_str).unwrap_or("upload.shw");

    // BYTES, NOT TEXT: a .shw is often a zip, and decoding one as text
    // destroys it (see `classify_wwb_bytes`). `text` is for a plain-XML show.
    let bytes = match (job.get("text"), job.get("content_b64")) {
        (Some(Value::String(t)), _) => t.clone().into_bytes(),
        (_, Some(Value::String(b))) => match base64::engine::general_purpose::STANDARD.decode(b.trim()) {
            Ok(bytes) => bytes,
            Err(e) => return failure(job_id, &format!("content_b64 does not decode: {e}"), started),
        },
        _ => return failure(job_id, "the job carries neither text nor content_b64", started),
    };
    if bytes.len() > max_bytes {
        return failure(
            job_id,
            &format!("the show is {} bytes; this worker takes at most {max_bytes}", bytes.len()),
            started,
        );
    }

    match wwb_importer::classify_wwb_bytes(&bytes, filename) {
        Ok(parsed) => json!({
            "job_id": job_id,
            "tool": TOOL,
            "ok": true,
            "result": parsed,
            "error": null,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        }),
        Err(e) => failure(job_id, &e, started),
    }
}

fn failure(job_id: Value, error: &str, started: Instant) -> Value {
    json!({
        "job_id": job_id,
        "tool": TOOL,
        "ok": false,
        "result": null,
        "error": error,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    })
}

/// The job loop. Blocking; the runner starts it on its own thread and exits
/// the process if it ever returns, so Docker restarts the container.
///
/// Its own session rather than the runner's: a slow parse must not hold up
/// the config, the will or the self-test door.
pub fn run_job_agent(host: &str, port: u16) {
    let (job_topic, _) = baremetal_io::module_job(MODULE).expect("wireless_wwb is a legal module token");
    let (result_topic, retain) =
        baremetal_io::module_result(MODULE).expect("wireless_wwb is a legal module token");
    let max_bytes = max_job_bytes(CONFIG_INI);

    let client_id = format!("apk-worker-{MODULE}-{}", std::process::id());
    let mut options = MqttOptions::new(client_id, host, port);
    options.set_keep_alive(Duration::from_secs(30));
    // A job is a file. Base64 grows it by a third, so the ceiling allows that.
    let ceiling = max_bytes / 3 * 4 + 64 * 1024;
    options.set_max_packet_size(ceiling, ceiling);
    if let Some((username, password)) = apkaudio_contracts::broker_auth::credentials() {
        options.set_credentials(username, password);
    }

    let (client, mut connection) = Client::new(options, 32);
    println!("🧰 [{MODULE}] worker listening on {job_topic}, answering on {result_topic}");
    for event in connection.iter() {
        // Every event, ping responses included, is progress.
        apk_plugin_runner::tick();
        match event {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                // Subscribed on every ConnAck: a clean session forgets it.
                let _ = client.try_subscribe(job_topic.clone(), QoS::AtLeastOnce);
            }
            Ok(Event::Incoming(Packet::Publish(p))) if p.topic == job_topic => {
                let answer = handle_job(&p.payload, max_bytes);
                println!(
                    "🧰 [{MODULE}] job {} → ok={} channels={}",
                    answer["job_id"], answer["ok"], answer["result"]["total_channels"]
                );
                let _ = client.try_publish(result_topic.clone(), QoS::AtLeastOnce, retain, answer.to_string());
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("⚠️ [{MODULE}] broker session: {e}; retrying");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
