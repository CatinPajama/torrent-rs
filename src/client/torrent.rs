use super::tracker::{PeerIps, TrackerRequest, TrackerResponse};
use serde_bencode::Error;
use serde_bencode::{de, value::Value};
use serde_bytes::ByteBuf;
use serde_derive::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::fs::File;
use std::net::Ipv4Addr;
use std::{fs, io::Read};

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    pub name: String,
    pub pieces: ByteBuf,
    #[serde(rename = "piece length")]
    pub piece_length: i64,
    #[serde(default)]
    pub md5sum: Option<String>,
    #[serde(default)]
    pub length: i64,
    #[serde(default)]
    pub private: Option<u8>,
    #[serde(default)]
    pub path: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "root hash")]
    pub root_hash: Option<String>,
}

#[allow(unused)]
#[derive(Debug, Deserialize)]
pub struct TorrentFile {
    pub info: Info,
    pub announce: String,
    #[serde(rename = "announce list")]
    announce_list: Option<Vec<Vec<String>>>,
    encoding: Option<String>,
    #[serde(rename = "creation date")]
    creation_date: Option<i64>,
    comment: Option<String>,
    #[serde(rename = "created by")]
    created_by: Option<String>,
}

impl TorrentFile {
    pub fn read_bytes(contents: &[u8]) -> Result<TorrentFile, Error> {
        de::from_bytes::<TorrentFile>(contents)
    }

    pub fn info_hash(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut hasher = Sha1::new();
        let serialized = serde_bencode::to_bytes(&self.info)?;

        hasher.update(&serialized);

        let hash = hasher.finalize();

        //let encoded_str: String = url::form_urlencoded::byte_serialize(&hash).collect();

        Ok(hash.into_iter().collect::<Vec<u8>>())
        //Ok(hash.iter().map(|x| format!("{:x}", x)).collect())
        //Ok(format!("{:X}", hash))
    }
}

pub struct Torrent {
    pub peer_id: Vec<u8>,
    pub torrent_file: TorrentFile,
    pub info_hash: Vec<u8>,
    //    pub peer_ips: Vec<String>,
    pub port: u32,
}

impl Torrent {
    pub fn new(
        path: &str,
        port: u32,
        peer_id: Vec<u8>,
    ) -> Result<Torrent, Box<dyn std::error::Error>> {
        let mut file = fs::File::open(path).unwrap();
        let mut contents = vec![];
        file.read_to_end(&mut contents).unwrap();
        let torrent_file = de::from_bytes::<TorrentFile>(&contents).unwrap();
        // let mut peer_hasher = sha1::Sha1::new();
        // peer_hasher.update(peer_id);
        // let peer_hash = peer_hasher.finalize().into_iter().collect::<Vec<u8>>();

        let mut info_hasher = Sha1::new();
        let serialized = serde_bencode::to_bytes(&torrent_file.info)?;

        info_hasher.update(&serialized);

        let info_hash = info_hasher.finalize();

        //let encoded_str: String = url::form_urlencoded::byte_serialize(&hash).collect();

        Ok(Torrent {
            // TODO
            port,
            torrent_file,
            info_hash: info_hash.into_iter().collect::<Vec<u8>>(),
            peer_id,
            // peer_ips: vec![],
        })
    }

    pub fn gen_tracker_request(
        &self,
        downloaded: i64,
        uploaded: i64,
        left: i64,
        event: Option<String>,
    ) -> Result<TrackerRequest, Box<dyn std::error::Error>> {
        Ok(TrackerRequest {
            announce_url: self.torrent_file.announce.clone(),
            compact: 1,
            info_hash: url::form_urlencoded::byte_serialize(&self.torrent_file.info_hash()?)
                .collect(),
            downloaded,
            uploaded,
            //left: self.torrent_file.info.length,
            left,
            peer_id: url::form_urlencoded::byte_serialize(&self.peer_id).collect(),
            port: self.port as i64,
            event,
        })
    }
    pub fn handle_tracker_response(tracker_response: TrackerResponse) -> Vec<String> {
        let mut peer_ips = vec![];
        match tracker_response.peers {
            PeerIps::Dict(p) => {
                for l in p {
                    let ip = match &l["ip"] {
                        Value::Bytes(s) => s.iter().map(|c| *c as char).collect::<String>(),
                        _ => String::new(),
                    };
                    let port = match &l["port"] {
                        Value::Int(x) => x.to_string(),
                        _ => String::new(),
                    };
                    peer_ips.push(format!("{}:{}", ip, port));
                }
            }
            PeerIps::BinaryModel(s) => {
                let ip = Ipv4Addr::new(s[0], s[1], s[2], s[3]);
                let port = u16::from_be_bytes([s[4], s[5]]);
                peer_ips.push(format!("{}:{}", ip, port));
            }
        }
        peer_ips
    }
    pub async fn send(
        &mut self,
        tracker_request: TrackerRequest,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // let tracker_request = self.gen_tracker_request()?;
        let url = tracker_request.url()?;
        let body = reqwest::get(&url).await?.bytes().await?;
        let tracker_response: TrackerResponse = de::from_bytes(&body)?;
        Ok(Self::handle_tracker_response(tracker_response))
    }
}
