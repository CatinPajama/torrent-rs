mod action;
mod file;
mod peer;
mod peer_manager;
mod torrent;
mod tracker;

use peer_manager::PeerManagerHandle;
use torrent::Torrent;

pub async fn run() {
    let mut torrent = Torrent::new("assets/thermo.torrent").unwrap();
    torrent.send().await.unwrap();
    let peer_manager_handle = PeerManagerHandle::new(torrent).await;
}
