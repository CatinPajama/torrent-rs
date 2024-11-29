use crate::client::action::{FileAction, FileMessage};

use super::action::{Message, PeerManagerAction};
use super::file::{verify, FileManagerHandle};
use super::peer::{create_peer, PeerWriterHandle};
use super::server::ServerActor;
use super::torrent::Torrent;
use super::BLOCK_SIZE;
use log::info;
use rand::seq::IteratorRandom;
use rand::thread_rng;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::fmt::Binary;
use std::path::Path;
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

fn add_peer_to_manager(
    peer_ips: Vec<String>,
    sender: mpsc::Sender<Action>,
    peer_id: Vec<u8>,
    info_hash: Vec<u8>,
    have_bitfield: Vec<bool>,
) {
    for peer_ip in peer_ips {
        let ip = peer_ip.clone();
        let sender = sender.clone();
        let peer_id = peer_id.clone();
        let info_hash = info_hash.clone();
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

                let _ = peer_writer_handle.sender.send(Message::Unchoke).await;
                let _ = sender
                    .send(Action {
                        id: peer_ip.clone(),
                        message: PeerManagerAction::AddPeer(peer_writer_handle),
                    })
                    .await;
                //_ = interval.tick() => {}
            }
        });
    }
}

impl PeerManagerHandle {
    pub async fn new(mut torrent: Torrent, download_path: &str) -> PeerManagerHandle {
        let (have_bitfield, actual_length) = verify(&torrent, download_path);
        let tracker_request = torrent
            .gen_tracker_request(0, 0, actual_length, Some("started".to_string()))
            .unwrap();
        // let url = tracker_request.url().unwrap();
        // println!("{}", &url);
        let tracker_response = torrent.send(tracker_request).await.unwrap();
        let peer_ips = Torrent::handle_tracker_response(&tracker_response)
            .into_iter()
            .take(30)
            .collect();
        // println!("{:?}", have_bitfield);
        let (sender, receiver) = mpsc::channel(1000);
        let peer_manager_handle = PeerManagerHandle {
            sender: sender.clone(),
        };

        let file_handler = FileManagerHandle::new(
            Path::new(download_path)
                .join(&torrent.torrent_file.info.name)
                .to_str()
                .unwrap(),
            torrent.torrent_file.info.piece_length,
            peer_manager_handle.clone(),
        )
        .await;
        //let mut peer_handler = HashMap::new();
        //let mut peer_state = HashMap::new();
        add_peer_to_manager(
            peer_ips,
            sender.clone(),
            torrent.peer_id.clone(),
            torrent.info_hash.clone(),
            have_bitfield.clone(),
        );
        let piece_count = torrent.torrent_file.info.pieces.len() / 20;
        let server_actor = ServerActor {
            peer_manager_handle: peer_manager_handle.clone(),
        };
        ServerActor::run(
            server_actor,
            torrent.port,
            torrent.peer_id.clone(),
            torrent.info_hash.clone(),
            have_bitfield.clone(),
        )
        .await;
        let left: Vec<Vec<bool>> = (0..piece_count)
            .map(|p| {
                let mut piece_size = torrent.torrent_file.info.piece_length;
                if p == piece_count - 1 {
                    piece_size = torrent.torrent_file.info.length
                        - torrent.torrent_file.info.piece_length * (piece_count as i64 - 1);
                }
                let num_blocks = (piece_size + BLOCK_SIZE - 1) / BLOCK_SIZE;
                vec![have_bitfield[p]; num_blocks as usize]
            })
            .collect();
        let actor = PeerManagerActor {
            sender,
            peer_handler: HashMap::new(),
            peer_state: HashMap::new(),
            receiver,
            file_handler,
            bitfield: vec![0; piece_count],
            have_bitfield,
            length: torrent.torrent_file.info.length,
            piece_length: torrent.torrent_file.info.piece_length,
            torrent,
            downloaded: 0,
            uploaded: 0,
            left,
            actual_length,
            interval: tracker_response.interval,
        };

        run_piece_manager_actor(actor).await;
        peer_manager_handle
    }
}

struct PeerManagerActor {
    receiver: tokio::sync::mpsc::Receiver<Action>,
    sender: tokio::sync::mpsc::Sender<Action>,
    peer_state: HashMap<String, (ChokedState, InterestedState, i64)>,
    peer_handler: HashMap<String, PeerWriterHandle>,
    file_handler: FileManagerHandle,
    bitfield: Vec<i64>,
    have_bitfield: Vec<bool>,
    length: i64,
    piece_length: i64,
    torrent: Torrent,
    downloaded: i64,
    uploaded: i64,
    left: Vec<Vec<bool>>,
    actual_length: i64,
    interval: i64,
}

async fn run_piece_manager_actor(mut actor: PeerManagerActor) {
    let mut choke_interval = interval(Duration::from_secs(10));
    let mut request_interval = interval(Duration::from_secs(1));
    let mut announce_interval = interval(Duration::from_secs(actor.interval as u64));
    announce_interval.tick().await;
    let mut keep_interval = interval(Duration::from_secs(120));
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
                        //info!("Request for piece {}",index);
                        if actor.have_bitfield[index as usize] {
                            let (sender, receiver) = oneshot::channel();
                            let _ = actor.file_handler.sender.send(FileMessage{sender, message : FileAction::Read(index,begin,size)}).await;
                            if let Ok(val) =  receiver.await {
                                    if let Some(sender) = actor.peer_handler.get_mut(&action.id) {
                                        let _ = sender.sender.send(Message::Piece(index,begin,val)).await;
                                        actor.uploaded += size as i64;
                                    }
                                }
                        }
                    },
                    //
                    _ => {}
                }
                    },
                    PeerManagerAction::Download(index,block) => {
                        actor.left[index as usize][block as usize] = true;
                        actor.have_bitfield[index as usize] = actor.left[index as usize].iter().all(|x| *x);

                        if actor.have_bitfield[index as usize] {
                            println!("Downloaded piece {}",index);
                        }
                    }
                    PeerManagerAction::AddPeer(write_handle) => {
                        info!("Peer Count {}",actor.peer_state.len());
                        actor.peer_handler.insert(action.id.clone(),write_handle);
                        actor.peer_state.insert(action.id,(ChokedState::Choked,InterestedState::NotInterested,0));
                    }
                    PeerManagerAction::RemovePeer => {
                        actor.peer_handler.remove(&action.id);
                        actor.peer_state.remove(&action.id);
                        info!("Peer Count {}",actor.peer_state.len());
                        //info!("Removed peer {}",action.id);
                    }
                    PeerManagerAction::Piece(index,begin,block,speed) => {
                        // TODO check piece hash
                        if let Some(value) = actor.peer_state.get_mut(&action.id) {
                                    value.2 = speed;
                        }
                        if !actor.have_bitfield[index as usize] {
                            actor.downloaded += block.len() as i64;
                            for peer_handler in actor.peer_handler.values().take(10) {
                                peer_handler.sender.send(Message::Have(index)).await;
                            }
                            let (sender, _) = oneshot::channel();
                            let _ = actor.file_handler.sender.send(FileMessage{sender, message : FileAction::Write(index,begin,block)}).await;
                        }
                    }
                }
            },
            _ = choke_interval.tick() => {

                let mut peer_sort_by_upload : Vec<(i64,&String)> = actor.peer_state.iter().map(|(k,v)| (v.2,k)).collect();
                peer_sort_by_upload.sort_by(|a, b| b.0.cmp(&a.0));
                for (_,value) in peer_sort_by_upload.iter().take(3) {
                    // println!("UNCHOKE MESSAGEG TO {}",value);
                    let _ = actor.peer_handler[*value].sender.send(Message::Unchoke).await;
                }
                for (_,value) in peer_sort_by_upload.iter().skip(3) {
                     let _ = actor.peer_handler[*value].sender.send(Message::Choke).await;
                }
                let mut rng = thread_rng();
                let rand_peer_opt = peer_sort_by_upload.iter().skip(3).choose(&mut rng);
                if let Some((_,rand_peer)) = rand_peer_opt {
                    let _ = actor.peer_handler[*rand_peer].sender.send(Message::Unchoke).await;
                }
            }
            _ = request_interval.tick() => {
                let max_heap = actor.bitfield.clone().into_iter().enumerate().filter(|x| !actor.have_bitfield[x.0 as usize]).map(|x| (Reverse(x.1),x.0)).collect::<BinaryHeap<(Reverse<i64>,usize)>>();
                for (_,best) in max_heap.iter().take(3) {
                    let mut size = actor.piece_length;
                    if *best == actor.have_bitfield.len() - 1 {
                        size = actor.length - (actor.bitfield.len() as i64 - 1) * size;
                    }
                    let mut downloaded = 0;
                    for peer_handle in actor.peer_handler.values().take(10) {
                        while downloaded < size {
                            let actual_size = std::cmp::min(std::cmp::min(size,BLOCK_SIZE),size - downloaded);
                            //let _ = peer_handle.sender.send(Message::Interested).await;
                            let _ = peer_handle.sender.send(Message::Request(*best as u32,downloaded as u32,actual_size as u32)).await;

                            downloaded += actual_size;
                        }
                    }
                }
            }
        _ = announce_interval.tick() => {
                let peer_cnt = actor.peer_handler.len();
                let size = actor.torrent.torrent_file.info.length;
                if let Ok(tracker_request) = actor.torrent.gen_tracker_request(actor.downloaded,actor.uploaded,actor.actual_length,None) {
                    // println!("{}",tracker_request.url().unwrap());
                    if let Ok(tracker_response) = actor.torrent.send(tracker_request).await {
                        actor.interval = tracker_response.interval;
                        let peer_ips = Torrent::handle_tracker_response(&tracker_response);
                         let peer_max_30 = peer_ips.into_iter().take(std::cmp::max(0,30-peer_cnt)).collect();
                          //  info!("{:?}",peer_max_30);
                        add_peer_to_manager(peer_max_30, actor.sender.clone(), actor.torrent.peer_id.clone(), actor.torrent.info_hash.clone(), actor.have_bitfield.clone());
                    }
                } else {
                    println!("err");
                }
           }
            _ = keep_interval.tick() => {
                for peer_handle in actor.peer_handler.values(){
                        let _ = peer_handle.sender.send(Message::KeepAlive).await;
                    }
            }
        }
    }
}
