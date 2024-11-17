mod action;
mod file;
mod peer;
mod peer_manager;
mod server;
mod torrent;
mod tracker;

use peer_manager::PeerManagerHandle;
use torrent::Torrent;

pub async fn run(path: &str, port: u32, peer_id: String) {
    let mut torrent = Torrent::new(path, port, peer_id).unwrap();
    torrent.send().await.unwrap();
    let peer_manager_handle = PeerManagerHandle::new(torrent).await;
}
