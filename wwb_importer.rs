// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Standalone Shure Wireless Workbench (.shw) Importer written in Rust. It does not
//! publish: the wireless_wwb worker (`SRC/src/lib.rs`) answers each job with the
//! result (LOOSE-datatype-datatype-wireless, LOOSE-datamigration-frames-to-workers).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WwbChannel {
    pub device_name: String,
    /// The device's `<manufacturer>`; `None` when the show does not say — never guessed.
    pub manufacturer: Option<String>,
    /// The device's `<model>`; `None` when the show does not say — never guessed.
    pub model: Option<String>,
    pub band: String,
    /// The `<channel number="…">` attribute; `None` when absent or not a number.
    pub channel_number: Option<u32>,
    pub channel_name: String,
    pub frequency_mhz: f64,
    pub zone: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WwbParseResult {
    pub status: String,
    pub source_format: String,
    pub file_path: String,
    pub total_channels: usize,
    /// `<channel>` blocks the show carried with no usable frequency.
    pub skipped_rows: usize,
    pub channels: Vec<WwbChannel>,
}

/// The four bytes every zip archive starts with (a local file header).
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

/// Parses a Shure Wireless Workbench .shw file — plain XML or a zipped .shw. No side effect.
pub fn parse_wwb_file(file_path: impl AsRef<Path>) -> Result<WwbParseResult, String> {
    let path = file_path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to open WWB file {:?}: {}", path, e))?;
    parse_wwb_bytes(&bytes, &path.to_string_lossy())
}

/// Parses the raw bytes of a .shw — what an uploaded `raw_blob` decodes to. No side effect.
/// Use this, not `parse_wwb_str`, for anything that may be a zip: text decoding destroys one.
pub fn parse_wwb_bytes(bytes: &[u8], source_name: &str) -> Result<WwbParseResult, String> {
    classify_wwb_bytes(bytes, source_name)
}

/// Parses .shw XML text. No side effect. Nothing in the running system calls it now
/// (the worker hands every job to `classify_wwb_bytes`); kept for library callers.
#[allow(dead_code)]
pub fn parse_wwb_str(content: &str, source_name: &str) -> Result<WwbParseResult, String> {
    Ok(classify_wwb_str(content, source_name))
}

/// No side effect. A zip (by its magic bytes) is unpacked and every member read as XML;
/// anything else is read as XML text. A zip that cannot be opened is an error, not an
/// empty show: lossy-decoding it as text would "succeed" with zero channels.
pub fn classify_wwb_bytes(bytes: &[u8], source_name: &str) -> Result<WwbParseResult, String> {
    if !bytes.starts_with(ZIP_MAGIC) {
        return Ok(classify_wwb_str(&String::from_utf8_lossy(bytes), source_name));
    }
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("WWB archive {:?} is not a readable zip: {}", source_name, e))?;
    let mut channels = Vec::new();
    let mut skipped = 0usize;
    for i in 0..archive.len() {
        let mut member = archive
            .by_index(i)
            .map_err(|e| format!("WWB archive {:?} member {}: {}", source_name, i, e))?;
        let mut content = String::new();
        if member.read_to_string(&mut content).is_ok() {
            skipped += parse_wwb_xml_content(&content, &mut channels);
        }
    }
    Ok(wwb_result(channels, skipped, source_name))
}

/// No side effect: .shw XML text to channels.
pub fn classify_wwb_str(content: &str, source_name: &str) -> WwbParseResult {
    let mut channels = Vec::new();
    let skipped = parse_wwb_xml_content(content, &mut channels);
    wwb_result(channels, skipped, source_name)
}

fn wwb_result(channels: Vec<WwbChannel>, skipped_rows: usize, source_name: &str) -> WwbParseResult {
    WwbParseResult {
        status: status_for(channels.len()),
        skipped_rows,
        source_format: "Shure_WWB_SHW".to_string(),
        file_path: source_name.to_string(),
        total_channels: channels.len(),
        channels,
    }
}

/// One piece of the document, in order: an element opening (name, attribute
/// text), an element closing, or character data (CDATA unwrapped, the five
/// predefined entities and numeric references decoded).
#[derive(Debug, PartialEq)]
enum XmlEvent<'a> {
    Open(&'a str, &'a str),
    Close(&'a str),
    Text(String),
}

/// A small XML scanner: tags, end tags, self-closing tags, CDATA; comments,
/// processing instructions and DOCTYPE skipped. It reads the markup, not the
/// line breaks, so a minified show reads the same as an indented one.
fn xml_events(content: &str) -> Vec<XmlEvent<'_>> {
    let mut out = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut out, rest);
            break;
        };
        push_text(&mut out, &rest[..lt]);
        rest = &rest[lt..];
        if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            out.push(XmlEvent::Text(body[..end].to_string()));
            rest = body.get(end + 3..).unwrap_or("");
        } else if let Some(body) = rest.strip_prefix("<!--") {
            rest = body.find("-->").map(|e| &body[e + 3..]).unwrap_or("");
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map(|e| &rest[e + 1..]).unwrap_or("");
        } else {
            let end = tag_end(rest);
            let tag = &rest[1..end];
            rest = rest.get(end + 1..).unwrap_or("");
            if let Some(name) = tag.strip_prefix('/') {
                out.push(XmlEvent::Close(name.trim()));
                continue;
            }
            let (tag, self_closing) = match tag.strip_suffix('/') {
                Some(t) => (t, true),
                None => (tag, false),
            };
            let split = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
            let (name, attrs) = (&tag[..split], &tag[split..]);
            out.push(XmlEvent::Open(name, attrs));
            if self_closing {
                out.push(XmlEvent::Close(name));
            }
        }
    }
    out
}

/// Index of the `>` closing the tag at the start of `s`, skipping any inside quoted attribute values.
fn tag_end(s: &str) -> usize {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return i,
            _ => {}
        }
    }
    s.len()
}

fn push_text<'a>(out: &mut Vec<XmlEvent<'a>>, raw: &str) {
    if !raw.trim().is_empty() {
        out.push(XmlEvent::Text(decode_entities(raw)));
    }
}

fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';').filter(|&i| i <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..semi];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The value of attribute `key` in a tag's attribute text.
fn attribute(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(pos) = rest.find(key) {
        let before_ok = rest[..pos].chars().last().is_none_or(char::is_whitespace);
        let after = rest[pos + key.len()..].trim_start();
        if before_ok {
            if let Some(v) = after.strip_prefix('=') {
                let v = v.trim_start();
                let q = v.chars().next()?;
                if q == '"' || q == '\'' {
                    let body = &v[1..];
                    return body.find(q).map(|e| decode_entities(&body[..e]));
                }
            }
        }
        rest = &rest[pos + key.len()..];
    }
    None
}

/// What the enclosing `<device>` says about itself — only what it says.
#[derive(Default, Clone)]
struct DeviceFields {
    manufacturer: Option<String>,
    model: Option<String>,
    device_name: String,
    band: String,
    zone: String,
}

/// Appends every channel with a usable frequency; returns how many `<channel>` blocks had none.
///
/// A device's fields are its own child elements, read wherever they sit
/// relative to its channels; a channel outside any `<device>` carries no
/// device fields. Nothing missing is filled in: no manufacturer is `None`,
/// not "Shure"; no `number=` is `None`, not the previous channel's number.
fn parse_wwb_xml_content(content: &str, channels: &mut Vec<WwbChannel>) -> usize {
    struct Pending {
        number: Option<u32>,
        name: String,
        freq_khz: Option<f64>,
    }
    let mut skipped = 0usize;
    let mut stack: Vec<&str> = Vec::new();
    // Each open <device>, innermost last, and the channels it holds, finished once it closes.
    let mut devices: Vec<(DeviceFields, Vec<Pending>)> = Vec::new();
    let mut channel: Option<Pending> = None;
    let mut loose: Vec<Pending> = Vec::new();
    let mut text = String::new();

    let finish = |dev: &DeviceFields, list: Vec<Pending>, channels: &mut Vec<WwbChannel>, skipped: &mut usize| {
        for ch in list {
            match ch.freq_khz.map(|k| k / 1000.0).filter(|m| *m > 100.0 && *m < 3000.0) {
                Some(frequency_mhz) => channels.push(WwbChannel {
                    device_name: dev.device_name.clone(),
                    manufacturer: dev.manufacturer.clone(),
                    model: dev.model.clone(),
                    band: dev.band.clone(),
                    channel_number: ch.number,
                    channel_name: ch.name,
                    frequency_mhz,
                    zone: dev.zone.clone(),
                }),
                None => *skipped += 1,
            }
        }
    };

    for event in xml_events(content) {
        match event {
            XmlEvent::Open(name, attrs) => {
                text.clear();
                if name == "device" {
                    devices.push((DeviceFields::default(), Vec::new()));
                } else if name == "channel" {
                    channel = Some(Pending {
                        number: attribute(attrs, "number").and_then(|n| n.trim().parse().ok()),
                        name: String::new(),
                        freq_khz: None,
                    });
                }
                stack.push(name);
            }
            XmlEvent::Text(t) => text.push_str(&t),
            XmlEvent::Close(name) => {
                // Pop to the matching open tag; a stray close tag is ignored.
                let Some(depth) = stack.iter().rposition(|n| *n == name) else { continue };
                stack.truncate(depth);
                let parent = stack.last().copied();
                let value = text.trim().to_string();
                text.clear();
                match (name, parent) {
                    ("channel", _) => {
                        if let Some(ch) = channel.take() {
                            match devices.last_mut() {
                                Some((_, list)) => list.push(ch),
                                None => loose.push(ch),
                            }
                        }
                    }
                    ("device", _) => {
                        if let Some((dev, list)) = devices.pop() {
                            finish(&dev, list, channels, &mut skipped);
                        }
                    }
                    ("channel_name", Some("channel")) => {
                        if let Some(ch) = channel.as_mut() {
                            ch.name = value;
                        }
                    }
                    ("frequency", Some("channel")) => {
                        if let Some(ch) = channel.as_mut() {
                            ch.freq_khz = value.parse().ok();
                        }
                    }
                    (field, Some("device")) => {
                        if let Some((dev, _)) = devices.last_mut() {
                            let present = Some(value.clone()).filter(|v| !v.is_empty());
                            match field {
                                "manufacturer" => dev.manufacturer = present,
                                "model" => dev.model = present,
                                "device_name" => dev.device_name = value,
                                "band" => dev.band = value,
                                "zone" => dev.zone = value,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // An unterminated document: what was read still counts, as it did line by line.
    if let Some(ch) = channel.take() {
        match devices.last_mut() {
            Some((_, list)) => list.push(ch),
            None => loose.push(ch),
        }
    }
    while let Some((dev, list)) = devices.pop() {
        finish(&dev, list, channels, &mut skipped);
    }
    finish(&DeviceFields::default(), loose, channels, &mut skipped);
    skipped
}

/// "success" only when the report yielded channels; a parse that found none is
/// "empty", never a success the panel would draw as an imported, blank table.
fn status_for(channels: usize) -> String {
    if channels > 0 { "success" } else { "empty" }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const SHOW: &str = include_str!("test_data/venue.shw");

    fn zipped(name: &str, body: &str) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zw.start_file(name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn a_zipped_show_reads_the_same_as_the_plain_one() {
        let plain = classify_wwb_str(SHOW, "venue.shw");
        assert!(plain.total_channels > 0, "fixture yields channels");
        let zip = zipped("venue.shw", SHOW);
        assert!(zip.starts_with(ZIP_MAGIC));
        let unzipped = classify_wwb_bytes(&zip, "venue.zip").unwrap();
        assert_eq!(unzipped.total_channels, plain.total_channels);
        assert_eq!(unzipped.channels[0].frequency_mhz, plain.channels[0].frequency_mhz);
    }

    #[test]
    fn plain_xml_bytes_still_parse() {
        let r = classify_wwb_bytes(SHOW.as_bytes(), "venue.shw").unwrap();
        assert_eq!(r.total_channels, classify_wwb_str(SHOW, "venue.shw").total_channels);
    }

    #[test]
    fn the_venue_fixture_yields_its_186_channels() {
        let r = classify_wwb_str(SHOW, "venue.shw");
        assert_eq!(r.total_channels, 186);
        let first = &r.channels[0];
        assert_eq!((first.manufacturer.as_deref(), first.model.as_deref()), (Some("Shure"), Some("PSM1000")));
        assert_eq!((first.device_name.as_str(), first.band.as_str(), first.channel_number), ("P10T", "G10", Some(1)));
    }

    #[test]
    fn a_minified_show_reads_the_same_as_the_indented_one() {
        let minified: String = SHOW.lines().map(str::trim).collect();
        assert_eq!(minified.lines().count(), 1);
        let a = classify_wwb_str(SHOW, "venue.shw");
        let b = classify_wwb_str(&minified, "venue.min.shw");
        assert_eq!(serde_json::to_string(&a.channels).unwrap(), serde_json::to_string(&b.channels).unwrap());
    }

    #[test]
    fn what_the_show_leaves_out_stays_out() {
        let xml = r#"<show><inventory><device><device_name><![CDATA[RX & 1]]></device_name>
            <channel><channel_name>A &amp; B</channel_name><frequency>470125</frequency></channel>
            <channel number="7"><frequency>471000</frequency></channel></device></inventory></show>"#;
        let r = classify_wwb_str(xml, "bare.shw");
        assert_eq!(r.total_channels, 2);
        let (a, b) = (&r.channels[0], &r.channels[1]);
        assert_eq!((a.manufacturer.clone(), a.model.clone()), (None, None), "no Shure / Generic invented");
        assert_eq!(a.channel_number, None, "no number= is no number, not 1");
        assert_eq!(b.channel_number, Some(7));
        assert_eq!((a.device_name.as_str(), a.channel_name.as_str()), ("RX & 1", "A & B"));
    }

    #[test]
    fn a_channel_after_its_device_closes_does_not_inherit_it() {
        let xml = r#"<show><device><manufacturer>Sennheiser</manufacturer><model>EM 6000</model>
            <channel number="1"><frequency>500000</frequency></channel></device>
            <channel number="2"><frequency>600000</frequency></channel></show>"#;
        let r = classify_wwb_str(xml, "two.shw");
        assert_eq!(r.channels[0].manufacturer.as_deref(), Some("Sennheiser"));
        assert_eq!(r.channels[1].manufacturer, None);
        assert_eq!(r.channels[1].model, None);
    }

    #[test]
    fn a_broken_zip_is_an_error_not_an_empty_show() {
        let mut bad = zipped("venue.shw", SHOW);
        bad.truncate(40);
        assert!(classify_wwb_bytes(&bad, "broken.zip").is_err());
    }
}
