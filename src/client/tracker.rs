use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum PeerIps {
    Dict(Vec<HashMap<String, serde_bencode::value::Value>>),
    BinaryModel(String),
}

#[derive(Serialize, Deserialize)]
pub struct TrackerResponse {
    #[serde(rename = "failure reason")]
    pub failure_reason: Option<String>,
    #[serde(rename = "interval")]
    pub interval: Option<i64>,
    #[serde(rename = "complete")]
    pub complete: Option<i64>,
    #[serde(rename = "warning reason")]
    pub warning_reason: Option<String>,
    #[serde(rename = "tracker id")]
    pub tracker_id: Option<String>,
    #[serde(rename = "incomplete")]
    pub incomplete: Option<i64>,
    #[serde(rename = "min interval")]
    pub min_interval: Option<i64>,
    #[serde(rename = "peers")]
    pub peers: PeerIps,
}

#[derive(Serialize)]
pub struct TrackerRequest {
    pub announce_url: String,
    pub info_hash: String,
    pub peer_id: String,
    pub port: i64,
    pub uploaded: i64,
    pub downloaded: i64,
    pub left: i64,
    pub compact: u8,
    pub event: String,
}

impl TrackerRequest {
    pub fn url(self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!("{}?peer_id={}&port={}&uploaded={}&downloaded={}&left={}&compact={}&event={}&info_hash={}",
        &self.announce_url,
        &self.peer_id,
        &self.port,
        &self.uploaded.to_string(),
        &self.downloaded.to_string(),
        &self.left.to_string(),
        &self.compact.to_string(),
        &self.event,
        &self.info_hash,
    ))
    }
}
