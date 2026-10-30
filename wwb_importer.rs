// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Standalone Shure Wireless Workbench (.shw) Importer written in Rust with MQTT publishing.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use apkaudio_contracts::topics::{self, MQTT_DEFAULT_PORT, MQTT_LOOPBACK_HOST};

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

/// Parses a Shure Wireless Workbench .shw file — plain XML or a zipped .shw — and publishes.
pub fn parse_wwb_file(file_path: impl AsRef<Path>) -> Result<WwbParseResult, String> {
    let path = file_path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to open WWB file {:?}: {}", path, e))?;
    parse_wwb_bytes(&bytes, &path.to_string_lossy())
}

/// Parses the raw bytes of a .shw — what an uploaded `raw_blob` decodes to — and publishes.
/// Use this, not `parse_wwb_str`, for anything that may be a zip: text decoding destroys one.
pub fn parse_wwb_bytes(bytes: &[u8], source_name: &str) -> Result<WwbParseResult, String> {
    let result = classify_wwb_bytes(bytes, source_name)?;
    publish_result(&result);
    Ok(result)
}

/// Parses .shw XML text and publishes.
pub fn parse_wwb_str(content: &str, source_name: &str) -> Result<WwbParseResult, String> {
    let result = classify_wwb_str(content, source_name);
    publish_result(&result);
    Ok(result)
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
    for i in 0..archive.len() {
        let mut member = archive
            .by_index(i)
            .map_err(|e| format!("WWB archive {:?} member {}: {}", source_name, i, e))?;
        let mut content = String::new();
        if member.read_to_string(&mut content).is_ok() {
            parse_wwb_xml_content(&content, &mut channels);
        }
    }
    Ok(wwb_result(channels, source_name))
}

/// No side effect: .shw XML text to channels.
pub fn classify_wwb_str(content: &str, source_name: &str) -> WwbParseResult {
    let mut channels = Vec::new();
    parse_wwb_xml_content(content, &mut channels);
    wwb_result(channels, source_name)
}

fn wwb_result(channels: Vec<WwbChannel>, source_name: &str) -> WwbParseResult {
    WwbParseResult {
        status: "success".to_string(),
        source_format: "Shure_WWB_SHW".to_string(),
        file_path: source_name.to_string(),
        mqtt_topic: wwb_mqtt_topic(),
        total_channels: channels.len(),
        channels,
    }
}

fn publish_result(result: &WwbParseResult) {
    if let Ok(json_payload) = serde_json::to_string_pretty(result) {
        publish_to_mqtt(&wwb_mqtt_topic(), &json_payload);
    }
}

fn parse_wwb_xml_content(content: &str, channels: &mut Vec<WwbChannel>) {
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
                if let Some(f_khz) = freq_khz {
                    let freq_mhz = f_khz / 1000.0;
                    if freq_mhz > 100.0 && freq_mhz < 3000.0 {
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

pub fn publish_to_mqtt(topic: &str, payload: &str) {
    use rumqttc::{Client, MqttOptions, Event, Packet, QoS};
    use std::time::Duration;

    let mut mqttoptions = MqttOptions::new("apkaudio-wwb-importer", MQTT_LOOPBACK_HOST, MQTT_DEFAULT_PORT);
    mqttoptions.set_keep_alive(Duration::from_secs(10));
    if let Some((username, password)) = apkaudio_contracts::broker_auth::credentials() {
        mqttoptions.set_credentials(username, password);
    }
    mqttoptions.set_max_packet_size(5_000_000, 5_000_000);

    let (client, mut connection) = Client::new(mqttoptions, 10);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut connected = false;
    let mut acked = false;

    for notification in connection.iter() {
        if let Ok(event) = notification {
            match event {
                Event::Incoming(Packet::ConnAck(_)) => {
                    connected = true;
                    let _ = client.publish(topic, QoS::AtLeastOnce, true, payload.as_bytes());
                    println!("[WWB MQTT PUBLISHER] Connected & Published payload to '{}'", topic);
                }
                Event::Incoming(Packet::PubAck(_)) => {
                    println!("[WWB MQTT PUBLISHER] Broker Acked payload on '{}'", topic);
                    acked = true;
                    break;
                }
                _ => {}
            }
        }
        if std::time::Instant::now() > deadline {
            break;
        }
    }
    if acked || connected {
        std::thread::sleep(Duration::from_millis(300));
    }
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
