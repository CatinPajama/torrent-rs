use bit_vec::BitVec;
use tokio::sync::oneshot;
pub enum Message {
    KeepAlive,
    Unchoke,
    Choke,
    Interested,
    NotInterested,
    Bitfield(BitVec),
    Request(u32, u32, u32),
    Have(u32),
    Piece(u32, u32, Vec<u8>),
    Cancel(u32, u32, u32),
    Port(u16),
}

pub enum PeerManagerAction {
    MessageAction(Message),
    UploadSpeed(f64),
    Download(u32),
}

pub struct FileMessage {
    pub message: FileAction,
    pub sender: oneshot::Sender<Vec<u8>>,
}

pub enum FileAction {
    Read(u32, u32, u32),
    Write(u32, u32, Vec<u8>),
}
