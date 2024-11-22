use crate::client::action::{FileAction, FileMessage};

use super::action::{Message, PeerManagerAction};
use super::file::FileManagerHandle;
use super::peer::{create_peer, PeerWriterHandle};
use super::server::ServerActor;
use super::torrent::Torrent;
use bit_vec::BitVec;
use rand::seq::IteratorRandom;
use rand::thread_rng;
use std::collections::HashMap;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio::{select, time::interval};

struct PeerHandle;

enum ChokedState {
    Unchoked,
    Choked,
}

enum InterestedState {
    Interested,
    NotInterested,
}

pub struct Action {
    pub id: String,
    pub message: PeerManagerAction,
}

/*
struct PeerActorHandler {
    sender: mpsc::Sender<Message>,
}
*/
#[derive(Clone)]
pub struct PeerManagerHandle {
    pub sender: mpsc::Sender<Action>,
}

impl PeerManagerHandle {
    pub async fn new(mut torrent: Torrent) -> PeerManagerHandle {
        let have_bitfield = torrent.verify();
        let tracker_request = torrent.gen_tracker_request(0, 0, 0).unwrap();
        torrent.send(tracker_request).await.unwrap();
        // println!("{:?}", have_bitfield);
        println!(
            "PIECE LENGTH : {} LENGTH : {} COUNT : {}",
            torrent.torrent_file.info.piece_length,
            torrent.torrent_file.info.length,
            torrent.torrent_file.info.pieces.len() / 20
        );
        let (sender, receiver) = mpsc::channel(100);
        let peer_manager_handle = PeerManagerHandle {
            sender: sender.clone(),
        };
        let file_handler = FileManagerHandle::new(
            &torrent.torrent_file.info.name,
            torrent.torrent_file.info.piece_length,
            peer_manager_handle.clone(),
        )
        .await;
        //let mut peer_handler = HashMap::new();
        //let mut peer_state = HashMap::new();
        for peer_ip in torrent.peer_ips.clone().into_iter() {
            let ip = peer_ip.clone();
            println!("{}", ip);
            let sender = sender.clone();
            let peer_id = torrent.peer_id.clone();
            let info_hash = torrent.info_hash.clone();
            let temp_have_bitfield = have_bitfield.clone();
            //let mut interval = interval(Duration::from_secs(100));
            tokio::spawn(async move {
                let stream_ = TcpStream::connect(&ip).await;
                if stream_.is_err() {
                    return;
                }
                let stream = stream_.unwrap();
                if let Ok(Ok((_, peer_writer_handle))) = timeout(
                    Duration::from_secs(30),
                    create_peer(stream, ip.clone(), sender.clone(), peer_id, info_hash),
                )
                .await
                {
                    let _ = peer_writer_handle
                        .sender
                        .send(Message::Bitfield(
                            temp_have_bitfield.clone().into_iter().collect(),
                        ))
                        .await;

                    let _ = peer_writer_handle.sender.send(Message::Interested).await;
                    let _ = sender
                        .send(Action {
                            id: peer_ip.clone(),
                            message: PeerManagerAction::AddPeer(peer_writer_handle),
                        })
                        .await;
                    //_ = interval.tick() => {}
                } else {
                    sender.send(Action {
                        id : peer_ip.clone(),
                        message : PeerManagerAction::RemovePeer,
                    }).await;
                }
            });
        }
        let piece_count = torrent.torrent_file.info.pieces.len() / 20;
        println!("piece count {}", piece_count);
        let server_actor = ServerActor {
            peer_manager_handle: peer_manager_handle.clone(),
        };
        ServerActor::run(
            server_actor,
            torrent.port,
            torrent.peer_id.clone(),
            torrent.info_hash.clone(),
        )
        .await;
        let actor = PeerManagerActor {
            peer_handler: HashMap::new(),
            peer_state: HashMap::new(),
            receiver,
            file_handler,
            bitfield: vec![0; piece_count],
            have_bitfield,
            length: torrent.torrent_file.info.length,
            piece_length: torrent.torrent_file.info.piece_length,
            torrent,
        };

        run_piece_manager_actor(actor).await;
        peer_manager_handle
    }
}

struct PeerManagerActor {
    receiver: tokio::sync::mpsc::Receiver<Action>,
    peer_state: HashMap<String, (ChokedState, InterestedState, i64)>,
    peer_handler: HashMap<String, PeerWriterHandle>,
    file_handler: FileManagerHandle,
    bitfield: Vec<i64>,
    have_bitfield: Vec<bool>,
    length: i64,
    piece_length: i64,
    torrent: Torrent,
}

fn calc_left(actor: &PeerManagerActor) -> i64 {
    0
}

async fn run_piece_manager_actor(mut actor: PeerManagerActor) {
    let mut choke_interval = interval(Duration::from_secs(20));
    let mut request_interval = interval(Duration::from_secs(10));
    let mut announce_interval = interval(Duration::from_secs(30));
    loop {
        select! {
            Some(action) = actor.receiver.recv() => {
                match action.message {
                    PeerManagerAction::MessageAction(message) => {
                        match message {
                    Message::Unchoke => {
                        if let Some(value) = actor.peer_state.get_mut(&action.id) {
                            value.0 = ChokedState::Unchoked;
                        }
                    },
                    Message::Choke => {
                        if let Some(value) = actor.peer_state.get_mut(&action.id) {
                            value.0 = ChokedState::Choked;
                        }
                    },
                    Message::Interested => {
                        if let Some(value) = actor.peer_state.get_mut(&action.id) {
                            value.1 = InterestedState::Interested;
                        }
                    }
                    Message::NotInterested => {
                        if let Some(value) = actor.peer_state.get_mut(&action.id) {
                            value.1 = InterestedState::NotInterested;
                        }
                    }

                    Message::Bitfield(bitfield) => {
                        for (idx,b) in bitfield.iter().enumerate() {
                            if b {
                                actor.bitfield[idx] += 1;
                            }
                        }
                    }
                    Message::Have(index) => {
                        actor.bitfield[index as usize] += 1;
                    }
                    Message::Request(index,begin,size) => {
                        println!("Request for piece {}",index);
                        if actor.have_bitfield[index as usize] {
                            // TODO tokio spawn this part
                            let (sender, receiver) = oneshot::channel();
                            let _ = actor.file_handler.sender.send(FileMessage{sender, message : FileAction::Read(index,begin,size)}).await;
                            match receiver.await {
                                Ok(val) => {
                                    if let Some(sender) = actor.peer_handler.get_mut(&action.id) {
                                        let _ = sender.sender.send(Message::Piece(index,begin,val)).await;
                                    }
                                }
                                Err(e) => {
                                            println!("{}",e);
                                }
                            }
                        }
                        // ask the file handler to send Piece message
                    },
                    //
                    _ => {}
                }
                    },
                    // PeerManagerAction::UploadSpeed(speed) => {
                    //     if let Some(value) = actor.peer_state.get_mut(&action.id) {
                    //         value.2 = speed;
                    //     }
                    // }
                    PeerManagerAction::Download(index) => {
                        actor.have_bitfield[index as usize] = true;
                    }
                    PeerManagerAction::AddPeer(write_handle) => {
                        println!("added peer");
                        actor.peer_handler.insert(action.id.clone(),write_handle);
                        actor.peer_state.insert(action.id,(ChokedState::Choked,InterestedState::NotInterested,0));
                    }
                    PeerManagerAction::RemovePeer => {
                        actor.peer_handler.remove(&action.id);
                        actor.peer_state.remove(&action.id);
                    }
                    PeerManagerAction::Piece(index,begin,block,speed) => {
                        // TODO check piece hash
                        if let Some(value) = actor.peer_state.get_mut(&action.id) {
                                    value.2 = speed;
                        }
                        if !actor.have_bitfield[index as usize] {
                            for peer_handler in actor.peer_handler.values().take(10) {
                                peer_handler.sender.send(Message::Have(index)).await;
                            }
                            println!("Downloaded piece {}",index);
                            let (sender, _) = oneshot::channel();
                            let _ = actor.file_handler.sender.send(FileMessage{sender, message : FileAction::Write(index,begin,block)}).await;
                        }
                    }
                }
            },
            _ = choke_interval.tick() => {

                let mut peer_sort_by_upload : Vec<(i64,&String)> = actor.peer_state.iter().map(|(k,v)| (v.2,k)).collect();
                peer_sort_by_upload.sort_by(|a, b| b.0.cmp(&a.0));
                for (_,value) in peer_sort_by_upload.iter().take(20) {
                    println!("UNCHOKE MESSAGEG TO {}",value);
                    let _ = actor.peer_handler[*value].sender.send(Message::Unchoke).await;
                }
                let mut rng = thread_rng();
                let rand_peer_opt = peer_sort_by_upload.iter().skip(3).choose(&mut rng);
                if let Some((_,rand_peer)) = rand_peer_opt {
                    let _ = actor.peer_handler[*rand_peer].sender.send(Message::Unchoke).await;
                }
            }
            _ = request_interval.tick() => {
                let mut lowest = i64::MAX;
                let mut best = 0;
                for (idx,b) in actor.bitfield.iter().enumerate() {
                    if *b < lowest && !actor.have_bitfield[idx] {
                        lowest = *b;
                        best = idx;
                    }
                }
                if !actor.have_bitfield[best] {
                    let mut size = actor.piece_length;
                    if best == actor.have_bitfield.len() - 1 {
                        size = actor.length - (actor.bitfield.len() as i64 - 1) * size;
                    }
                    let actual_size = std::cmp::min(size,1 << 14);
                    println!("BEST PIECE TO DOWNLOA : {} PIECE SIZE {}",best,actual_size);
                    for peer_handle in actor.peer_handler.values().take(10) {
                        let _ = peer_handle.sender.send(Message::Request(best as u32,0,actual_size as u32)).await;

                        let remain = size - actual_size;
                        if remain != 0 {
                            let _ = peer_handle.sender.send(Message::Request(best as u32,actual_size as u32, remain as u32)).await;
                         }
                    }
                }
            }
        _ = announce_interval.tick() => {
                let tracker_request = actor.torrent
            .gen_tracker_request(0, 0, calc_left(&actor))
            .unwrap();
        actor.torrent.send(tracker_request).await.unwrap();


        }
        }
    }
}
