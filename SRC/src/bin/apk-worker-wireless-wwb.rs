// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Container entry point: the runner's five duties plus the job loop.
apk_plugin_runner::main!(kind = "worker", token = "wireless_wwb", lib = apk_worker_wireless_wwb, run = [run_job_agent]);
