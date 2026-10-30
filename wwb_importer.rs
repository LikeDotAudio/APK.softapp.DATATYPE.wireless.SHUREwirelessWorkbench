// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Standalone Shure Wireless Workbench (.shw) Importer written in Rust. It does not
//! publish: the data-migration daemon publishes the result on the broker it was
//! started with (LOOSE-datatype-datatype-wireless).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use apkaudio_contracts::topics;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WwbChannel {
    pub device_name: String,
    pub manufacturer: String,
    pub model: String,
    pub band: String,
    pub channel_number: u32,
    pub channel_name: String,
    pub frequency_mhz: f64,
    pub zone: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WwbParseResult {
    pub status: String,
    pub source_format: String,
    pub file_path: String,
    pub mqtt_topic: String,
    pub total_channels: usize,
    /// `<channel>` blocks the show carried with no usable frequency.
    pub skipped_rows: usize,
    pub channels: Vec<WwbChannel>,
}

/// `APK.audio/System/DataMigration/WWB/ImportedData`, built from the contract
/// rather than typed out — see `contracts/rust/src/topics.rs`, whose own header
/// names the DataMigration importers as the literals it exists to replace.
pub fn wwb_mqtt_topic() -> String {
    topics::data_migration("WWB", "ImportedData")
        .expect("WWB/ImportedData are literal segments the contract accepts")
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

/// Parses .shw XML text. No side effect. Called by the data-migration daemon for a
/// `raw_content` payload; the standalone runner, which compiles this file too, does not.
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
        mqtt_topic: wwb_mqtt_topic(),
        total_channels: channels.len(),
        channels,
    }
}

/// Appends every channel with a usable frequency; returns how many `<channel>` blocks had none.
fn parse_wwb_xml_content(content: &str, channels: &mut Vec<WwbChannel>) -> usize {
    let mut skipped = 0usize;
    let mut current_manufacturer = "Shure".to_string();
    let mut current_model = "Generic".to_string();
    let mut current_device_name = "".to_string();
    let mut current_band = "".to_string();
    let mut current_zone = "".to_string();

    let mut in_device = false;
    let mut in_channel = false;
    let mut channel_num = 1;
    let mut ch_name = "".to_string();
    let mut freq_khz: Option<f64> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<device>") {
            in_device = true;
            current_manufacturer = "Shure".to_string();
            current_model = "Generic".to_string();
            current_device_name = "".to_string();
            current_band = "".to_string();
            current_zone = "".to_string();
        } else if trimmed.starts_with("</device>") {
            in_device = false;
        } else if trimmed.starts_with("<channel ") {
            in_channel = true;
            ch_name = "".to_string();
            freq_khz = None;
            if let Some(pos) = trimmed.find("number=\"") {
                let rest = &trimmed[pos + 8..];
                if let Some(end) = rest.find('"') {
                    channel_num = rest[..end].parse::<u32>().unwrap_or(1);
                }
            }
        } else if trimmed.starts_with("</channel>") {
            if in_channel {
                let usable = freq_khz.map(|k| k / 1000.0).filter(|m| *m > 100.0 && *m < 3000.0);
                if usable.is_none() {
                    skipped += 1;
                }
                if let Some(freq_mhz) = usable {
                    {
                        channels.push(WwbChannel {
                            device_name: current_device_name.clone(),
                            manufacturer: current_manufacturer.clone(),
                            model: current_model.clone(),
                            band: current_band.clone(),
                            channel_number: channel_num,
                            channel_name: ch_name.clone(),
                            frequency_mhz: freq_mhz,
                            zone: current_zone.clone(),
                        });
                    }
                }
            }
            in_channel = false;
        }

        if in_device && !in_channel {
            if trimmed.starts_with("<manufacturer") {
                current_manufacturer = extract_xml_cdata(trimmed);
            } else if trimmed.starts_with("<model") {
                current_model = extract_xml_cdata(trimmed);
            } else if trimmed.starts_with("<device_name") {
                current_device_name = extract_xml_cdata(trimmed);
            } else if trimmed.starts_with("<band") {
                current_band = extract_xml_text(trimmed);
            } else if trimmed.starts_with("<zone") {
                current_zone = extract_xml_text(trimmed);
            }
        } else if in_channel {
            if trimmed.starts_with("<channel_name") {
                ch_name = extract_xml_cdata(trimmed);
            } else if trimmed.starts_with("<frequency") {
                let freq_str = extract_xml_text(trimmed);
                if let Ok(f) = freq_str.parse::<f64>() {
                    freq_khz = Some(f);
                }
            }
        }
    }
    skipped
}

fn extract_xml_cdata(line: &str) -> String {
    if let Some(start) = line.find("<![CDATA[") {
        let rest = &line[start + 9..];
        if let Some(end) = rest.find("]]>") {
            return rest[..end].to_string();
        }
    }
    extract_xml_text(line)
}

fn extract_xml_text(line: &str) -> String {
    if let Some(start) = line.find('>') {
        let rest = &line[start + 1..];
        if let Some(end) = rest.find('<') {
            return rest[..end].to_string();
        }
    }
    "".to_string()
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
    fn a_broken_zip_is_an_error_not_an_empty_show() {
        let mut bad = zipped("venue.shw", SHOW);
        bad.truncate(40);
        assert!(classify_wwb_bytes(&bad, "broken.zip").is_err());
    }
}
