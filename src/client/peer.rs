use std::error::Error;

use super::action::{Message, PeerManagerAction};
use super::peer_manager::Action;
use bit_vec::BitVec;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::TcpStream;
use tokio::select;
use tokio::time::Instant;
use tokio::{net::tcp::OwnedWriteHalf, sync::mpsc};
use tokio_util::sync::CancellationToken;

pub struct PeerWriterActor {
    receiver: mpsc::Receiver<Message>,
    writer: OwnedWriteHalf,
}

async fn run_peer_writer_actor(
    mut actor: PeerWriterActor,
    peer_id: Vec<u8>,
    info_hash: Vec<u8>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let stream = &mut actor.writer;

    stream.write_all(&[19]).await?;
    stream.write_all(b"BitTorrent protocol").await?;
    stream.write_all(&[0; 8]).await?;
    stream.write_all(&info_hash).await?;
    stream.write_all(&peer_id).await?;

    while let Some(message) = actor.receiver.recv().await {
        match message {
            Message::KeepAlive => {
                stream.write_u32(0).await?;
            }
            Message::Choke => {
                stream.write_u32(1).await?;
                stream.write_u8(0).await?;
            }
            Message::Unchoke => {
                stream.write_u32(1).await?;
                stream.write_u8(1).await?;
            }
            Message::Interested => {
                stream.write_u32(1).await?;
                stream.write_u8(2).await?;
            }
            Message::NotInterested => {
                stream.write_u32(1).await?;
                stream.write_u8(3).await?;
            }
            Message::Have(ref piece_index) => {
                stream.write_u32(5).await?;
                stream.write_u8(4).await?;
                stream.write_u32(*piece_index).await?;
            }
            Message::Bitfield(bitfield) => {
                stream
                    .write_u32((bitfield.len() as u32).div_ceil(8) + 1)
                    .await?;
                stream.write_u8(5).await?;
                stream.write_all(&bitfield.to_bytes()).await?;
            }
            Message::Request(index, begin, length) => {
                stream.write_u32(13).await?;
                stream.write_u8(6).await?;
                stream.write_u32(index).await?;
                stream.write_u32(begin).await?;
                stream.write_u32(length).await?;
            }
            Message::Piece(index, begin, block) => {
                stream.write_u32(9 + block.len() as u32).await?;
                stream.write_u8(7).await?;
                stream.write_u32(index).await?;
                stream.write_u32(begin).await?;
                stream.write_all(&block).await?;
            }
            Message::Cancel(index, begin, length) => {
                stream.write_u32(13).await?;
                stream.write_u8(8).await?;
                stream.write_u32(index).await?;
                stream.write_u32(begin).await?;
                stream.write_u32(length).await?;
            }

            Message::Port(listen_port) => {
                stream.write_u32(3).await?;
                stream.write_u8(9).await?;
                stream.write_u16(listen_port).await?;
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct PeerWriterHandle {
    pub sender: mpsc::Sender<Message>,
}

impl PeerWriterHandle {
    fn new(
        writer: OwnedWriteHalf,
        token: CancellationToken,
        peer_id: Vec<u8>,
        info_hash: Vec<u8>,
    ) -> PeerWriterHandle {
        let (sender, receiver) = mpsc::channel(1000);
        let actor = PeerWriterActor { receiver, writer };
        tokio::spawn(async move {
            select! {
                err = run_peer_writer_actor(actor,peer_id,info_hash) => {
                    if err.is_err() {
                        token.cancel();
                        //println!("PEER WRITER ERR {}", err.unwrap_err());
                    }
                }
                _ = token.cancelled() => {
                        //println!("writer also got cancelled");
                }
            }
        });
        PeerWriterHandle { sender }
    }
}

pub struct PeerReaderActor {
    receiver: mpsc::Receiver<Message>,
    reader: OwnedReadHalf,
}

#[derive(Clone)]
pub struct PeerReaderHandle {
    sender: mpsc::Sender<Message>,
}

impl PeerReaderHandle {
    fn new(
        reader: OwnedReadHalf,
        ip: String,
        peer_manager_sender: mpsc::Sender<Action>,
        token: CancellationToken,
    ) -> PeerReaderHandle {
        let (sender, receiver) = mpsc::channel(1000);
        let actor = PeerReaderActor { receiver, reader };
        let peer_manager_sender_clone = peer_manager_sender.clone();
        let ip_clone = ip.clone();
        tokio::spawn(async move {
            select! {
                err = run_peer_reader_actor(actor, ip, peer_manager_sender) => {
                    if err.is_err() {
                        peer_manager_sender_clone.send(Action {
                            id : ip_clone,
                            message : PeerManagerAction::RemovePeer,
                        }).await.unwrap();
                        token.cancel();
                    }
                }
                _ = token.cancelled() => {
                        peer_manager_sender_clone.send(Action {
                            id : ip_clone,
                            message : PeerManagerAction::RemovePeer,
                        }).await.unwrap();
                }
            }
        });
        PeerReaderHandle { sender }
    }
}

pub async fn create_peer(
    mut stream: TcpStream,
    ip: String,
    peer_manager_sender: mpsc::Sender<Action>,
    peer_id: Vec<u8>,
    info_hash: Vec<u8>,
) -> Result<(PeerReaderHandle, PeerWriterHandle), Box<dyn Error + Send + Sync>> {
    //let stream_ = TcpStream::connect(&ip).await;
    //let mut stream = stream_?;
    /*
    let length = stream.read_u8().await?;
    let mut pstr: Vec<u8> = vec![0; length as usize];
    stream.read_exact(&mut pstr).await?;
    let mut reserved: [u8; 8] = [0; 8];
    stream.read_exact(&mut reserved).await?;
    let mut info_hash: [u8; 20] = [0; 20];
    let mut peer_id: [u8; 20] = [0; 20];
    stream.read_exact(&mut info_hash).await?;
    stream.read_exact(&mut peer_id).await?;

    stream.write_all(&[19]).await?;
    stream.write_all(b"BitTorrent protocol").await?;
    stream.write_all(&[0; 8]).await?;
    stream.write_all(&info_hash).await?;
    stream.write_all(&peer_id).await?;
    */

    let (reader, writer) = stream.into_split();
    let token = CancellationToken::new();
    let peer_reader_handle = PeerReaderHandle::new(reader, ip, peer_manager_sender, token.clone());
    let peer_writer_handle = PeerWriterHandle::new(writer, token.clone(), peer_id, info_hash);
    Ok((peer_reader_handle, peer_writer_handle))
}

async fn run_peer_reader_actor(
    actor: PeerReaderActor,
    ip: String,
    sender: mpsc::Sender<Action>,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut stream = actor.reader;

    let length = stream.read_u8().await?;
    let mut pstr: Vec<u8> = vec![0; length as usize];
    stream.read_exact(&mut pstr).await?;
    let mut reserved: [u8; 8] = [0; 8];
    stream.read_exact(&mut reserved).await?;
    let mut info_hash: [u8; 20] = [0; 20];
    let mut peer_id: [u8; 20] = [0; 20];
    stream.read_exact(&mut info_hash).await?;
    stream.read_exact(&mut peer_id).await?;

    loop {
        let mut length = stream.read_u32().await?;
        if length == 0 {
            continue;
            //return PeerManagerAction::MessageAction(Message::KeepAlive);
        }
        length -= 1;
        let id = stream.read_u8().await?;
        // println!("{} {}", id, length);
        let _ = sender
            .send(Action {
                id: ip.clone(),
                message: match id {
                    0 => PeerManagerAction::MessageAction(Message::Choke),
                    1 => PeerManagerAction::MessageAction(Message::Unchoke),
                    2 => PeerManagerAction::MessageAction(Message::Interested),
                    3 => PeerManagerAction::MessageAction(Message::NotInterested),
                    4 => PeerManagerAction::MessageAction(Message::Have(stream.read_u32().await?)),
                    5 => {
                        let mut bit_arr: Vec<u8> = vec![0; length as usize];
                        stream.read_exact(&mut bit_arr).await?;
                        PeerManagerAction::MessageAction(Message::Bitfield(BitVec::from_bytes(
                            &bit_arr,
                        )))
                    }
                    6 => {
                        let piece_index = stream.read_u32().await?;
                        let begin = stream.read_u32().await?;
                        let length = stream.read_u32().await?;

                        PeerManagerAction::MessageAction(Message::Request(
                            piece_index,
                            begin,
                            length,
                        ))
                    }
                    7 => {
                        let piece_index = stream.read_u32().await?;
                        let begin = stream.read_u32().await?;
                        length -= 4;
                        length -= 4;
                        let mut block: Vec<u8> = vec![0; length as usize];
                        let start_time = Instant::now();
                        stream.read_exact(&mut block).await?;
                        let duration = start_time.elapsed();
                        let speed = length as u128 / std::cmp::max(duration.as_millis(), 1);
                        PeerManagerAction::Piece(piece_index, begin, block, speed as i64)
                    }
                    8 => {
                        let piece_index = stream.read_u32().await?;
                        let begin = stream.read_u32().await?;
                        let length = stream.read_u32().await?;

                        PeerManagerAction::MessageAction(Message::Cancel(
                            piece_index,
                            begin,
                            length,
                        ))
                    }
                    9 => PeerManagerAction::MessageAction(Message::Port(stream.read_u16().await?)),
                    _ => {
                        panic!("JPT");
                    }
                },
            })
            .await;
    }
}
