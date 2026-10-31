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

# MARKER_HEADERS, the lifecycle and the logging call come from
# `apk_wireless_import` (DATATYPE.wireless.CSV/SRC/python, package
# `apk-wireless-import`, pinned in requirements.txt beside this file) — one copy
# for the IAS, SoundBase-PDF and Wireless Workbench edges. This edge no longer
# imports NODE.BareMetal's `importer_logging`, so it needs no other repository on
# PYTHONPATH.

from apk_wireless_import import MARKER_HEADERS, Lifecycle, log

try:
    from oaRustCore.showfile_unpacker import unpack_showfile
except ImportError:
    unpack_showfile = None

# "live" when the Rust unpacker is reachable, "unavailable" when it is not.
# Asserted by Tests/test_handler_status.py.
STATUS = "live" if unpack_showfile else "unavailable"


def convert_shure_wwb_shw_to_markers(file_path):
    if unpack_showfile:
        log("convert_shure_wwb_shw_to_markers", "🚀 Using HIGH-PERFORMANCE RUST Showfile unpacker.")
        return MARKER_HEADERS, unpack_showfile(str(file_path))
    raise RuntimeError("Rust Showfile parser required")


Marker_convert_WWB_SHW_File_report_to_csv = convert_shure_wwb_shw_to_markers
Marker_convert_wwb_zip_report_to_csv = convert_shure_wwb_shw_to_markers


def _self_test():
    """Unpack a one-member archive through the Rust unpacker and check what comes back."""
    import json
    import os
    import tempfile
    import zipfile
    member = {"self_test": True}
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "self_test.shw")
        with zipfile.ZipFile(path, "w") as zf:
            zf.writestr("show.json", json.dumps(member))
        got = unpack_showfile(path)
    # the unpacker returns each member's JSON as a string (see the header)
    return isinstance(got, dict) and json.loads(got.get("show.json", "null")) == member


# The importer interface's lifecycle, from the shared module: start/stop are
# no-ops (stateless), run_tests() is False unless STATUS is "live" and the
# unpack above answers right — never True for having run nothing.
_life = Lifecycle(STATUS, self_test=_self_test)
start, stop, status, run_tests = _life.start, _life.stop, _life.status, _life.run_tests


# csv_to_json_and_publish() used to sit here: an empty function exported under a name
# that promised a publish. Nothing called it, and the publish lives in the Rust importer
# (wwb_importer.rs, parse_wwb_* -> publish_to_mqtt), so it is gone.
__all__ = ["convert_shure_wwb_shw_to_markers", "Marker_convert_WWB_SHW_File_report_to_csv", "Marker_convert_wwb_zip_report_to_csv", "MARKER_HEADERS", "STATUS", "start", "stop", "status", "run_tests"]
