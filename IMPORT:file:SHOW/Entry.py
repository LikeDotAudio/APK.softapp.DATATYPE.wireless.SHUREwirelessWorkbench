# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
# Reads a Shure Wireless Workbench showfile (.shw / .zip) and returns the JSON
# members DataMigration's WWB importer works from.
#
# Owns: the WWB archive dialect. `oaRustCore.showfile_unpacker` does the
# unzip and the JSON indexing; this file is the Python edge.
#
# Must not: unpack archives in Python. The Rust unpacker is the one
# implementation, and a fallback here would quietly become the real one.
#
# ⚠️ The unpacker returns each member's JSON as a **string**, not a parsed dict —
# its own comment says so ("Convert serde_json to python dict would be complex
# here"). Callers parse. Changing that is a change to this contract.
#
# The import name is a contract with `src/lib.rs`; see the note in
# `oaFileImportHTML/Entry.py`. This module spent its life importing
# `oa_showfile_unpacker_rs` while the directory was named
# `XXXoa_showfile_unpacker_rs` and therefore outside the crate's module tree, so
# every call raised while `status()` reported "Running with Rust Engine".

from importer_logging import matrix_log

MARKER_HEADERS = ["ZONE", "GROUP", "DEVICE", "NAME", "FREQ_MHZ", "PEAK"]

try:
    from oaRustCore.showfile_unpacker import unpack_showfile
except ImportError:
    unpack_showfile = None

# "live" when the Rust unpacker is reachable, "unavailable" when it is not.
# Asserted by Tests/test_handler_status.py.
STATUS = "live" if unpack_showfile else "unavailable"


# Inline comment: Logic for convert_shure_wwb_shw_to_markers
def convert_shure_wwb_shw_to_markers(file_path):
    if unpack_showfile:
        matrix_log("ui", "importer", "convert_shure_wwb_shw_to_markers", "🚀 Using HIGH-PERFORMANCE RUST Showfile unpacker.", "DEBUG")
        return MARKER_HEADERS, unpack_showfile(str(file_path))
    raise RuntimeError("Rust Showfile parser required")


Marker_convert_WWB_SHW_File_report_to_csv = convert_shure_wwb_shw_to_markers
Marker_convert_wwb_zip_report_to_csv = convert_shure_wwb_shw_to_markers


# Inline comment: Logic for csv_to_json_and_publish
def csv_to_json_and_publish(*args, **kwargs):
    pass


# Inline comment: Logic for start
def start(): pass
# Inline comment: Logic for stop
def stop(): pass
# Inline comment: Logic for status
def status(): return STATUS
# Inline comment: Logic for run_tests
def run_tests(): return True

__all__ = ["convert_shure_wwb_shw_to_markers", "Marker_convert_WWB_SHW_File_report_to_csv", "Marker_convert_wwb_zip_report_to_csv", "csv_to_json_and_publish", "MARKER_HEADERS", "STATUS", "start", "stop", "status", "run_tests"]
