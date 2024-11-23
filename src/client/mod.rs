mod action;
mod file;
mod peer;
mod peer_manager;
mod server;
mod torrent;
mod tracker;

use peer_manager::PeerManagerHandle;
use torrent::Torrent;

use rand::Rng;

const BLOCK_SIZE: i64 = 1 << 14;

fn generate_peer_id() -> String {
    let prefix = "-qB5000-";

    let random_part: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Uniform::new(0u8, 10))
        .take(12)
        .map(|n| (n + b'0') as char)
        .collect();

    format!("{}{}", prefix, random_part)
}

pub async fn run(path: &str, download_path: &str, port: u32) {
    let peer_id = generate_peer_id().into_bytes();
    let torrent = Torrent::new(path, port, peer_id).unwrap();
    // torrent.send().await.unwrap();
    let peer_manager_handle = PeerManagerHandle::new(torrent, download_path).await;
}
