// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Container entry point: the runner's five duties plus the job loop.
//!
//! `manifest = "agent.toml"` (ROAD-3472): the manifest is compiled in, and the
//! runner serves `GET /status` / `GET /telemetry` on 127.0.0.1:8228 — `running`
//! until the first job is taken, `serving` after.
apk_plugin_runner::main!(kind = "worker", token = "wireless_wwb", lib = apk_worker_wireless_wwb, run = [run_job_agent], manifest = "agent.toml");
