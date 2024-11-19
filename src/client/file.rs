use super::action::FileAction;
use super::action::FileMessage;
use super::action::PeerManagerAction;
use super::peer::PeerReaderHandle;
use super::peer_manager::Action;
use super::peer_manager::PeerManagerHandle;
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
        let (sender, receiver) = mpsc::channel(100);
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
                        message: PeerManagerAction::Download(index),
                    })
                    .await
                    .unwrap();
            }
        }
    }
}
