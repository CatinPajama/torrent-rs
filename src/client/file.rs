use std::io::Read;
use std::path::Path;

use super::action::FileAction;
use super::action::FileMessage;
use super::action::PeerManagerAction;
use super::peer::PeerReaderHandle;
use super::peer_manager::Action;
use super::peer_manager::PeerManagerHandle;
use super::torrent::Torrent;
use super::BLOCK_SIZE;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::{fs::File, fs::OpenOptions, sync::mpsc};

pub struct FileManagerHandle {
    pub sender: mpsc::Sender<FileMessage>,
}

impl FileManagerHandle {
    pub async fn new(
        file_path: &str,
        piece_size: i64,
        peer_manager_handle: PeerManagerHandle,
    ) -> FileManagerHandle {
        let stream = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file_path)
            .await
            .unwrap();
        let (sender, receiver) = mpsc::channel(1000);
        let actor = FileManagerActor {
            peer_manager_handle,
            receiver,
            stream,
            piece_size,
        };
        tokio::spawn(async move {
            run_file_manager_actor(actor).await;
        });
        FileManagerHandle { sender }
    }
}

struct FileManagerActor {
    receiver: mpsc::Receiver<FileMessage>,
    peer_manager_handle: PeerManagerHandle,
    stream: File,
    piece_size: i64,
}

pub fn verify(torrent: &Torrent, download_path: &str) -> Vec<bool> {
    let piece_count = torrent.torrent_file.info.pieces.len() / 20;
    let file = std::fs::File::open(
        Path::new(download_path)
            .join(&torrent.torrent_file.info.name)
            .to_str()
            .unwrap(),
    );

    let mut pieces = torrent.torrent_file.info.pieces.iter();
    let piece_size = torrent.torrent_file.info.piece_length;
    let length = torrent.torrent_file.info.length;
    match file {
        Ok(mut stream) => (0..piece_count)
            .map(move |p_index| {
                let buffer_size = if p_index < piece_count - 1 {
                    piece_size
                } else {
                    length - (piece_count as i64 - 1) * piece_size
                };
                let mut buffer = vec![0; buffer_size as usize];
                if stream.read_exact(&mut buffer).is_err() {
                    return false;
                }
                let mut hasher = sha1::Sha1::new();
                hasher.update(buffer);
                let val = hasher.finalize().iter().eq(pieces.clone().take(20));
                pieces.nth(19);
                val
            })
            .collect(),
        Err(_) => {
            vec![false; piece_count]
        }
    }
}

async fn run_file_manager_actor(mut actor: FileManagerActor) {
    while let Some(message) = actor.receiver.recv().await {
        match message.message {
            FileAction::Read(index, begin, size) => {
                let mut block: Vec<u8> = vec![0; size as usize];
                let _ = actor
                    .stream
                    .seek(SeekFrom::Start(
                        index as u64 * actor.piece_size as u64 + begin as u64,
                    ))
                    .await
                    .unwrap();
                let _ = actor.stream.read_exact(&mut block).await.unwrap();
                let _ = message.sender.send(block);
            }
            FileAction::Write(index, begin, block) => {
                let offset = begin;
                let block_index = offset as i64 / BLOCK_SIZE;
                actor
                    .stream
                    .seek(SeekFrom::Start(
                        index as u64 * actor.piece_size as u64 + begin as u64,
                    ))
                    .await
                    .unwrap();
                actor.stream.write_all(&block).await.unwrap();
                actor
                    .peer_manager_handle
                    .sender
                    .send(Action {
                        id: "123".to_string(),
                        message: PeerManagerAction::Download(index, block_index),
                    })
                    .await
                    .unwrap();
            }
        }
    }
}
