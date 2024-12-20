use log::info;
use tokio::net::TcpListener;

use crate::client::{
    action::{Message, PeerManagerAction},
    peer::create_peer,
};

use super::peer_manager::{Action, PeerManagerHandle};

pub struct ServerActor {
    pub peer_manager_handle: PeerManagerHandle,
}

impl ServerActor {
    pub async fn run(
        self,
        port: u32,
        peer_id: Vec<u8>,
        info_hash: Vec<u8>,
        have_bitfield: Vec<bool>,
    ) {
        let listener = TcpListener::bind(format!("::1:{}", port)).await.unwrap();

        let sender = self.peer_manager_handle.sender.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let ip = stream.peer_addr().unwrap().to_string();
                info!("{}", ip);
                if let Ok((_, peer_writer_handle)) = create_peer(
                    stream,
                    ip.clone(),
                    sender.clone(),
                    peer_id.clone(),
                    info_hash.clone(),
                )
                .await
                {
                    let _ = peer_writer_handle
                        .sender
                        .send(Message::Bitfield(
                            have_bitfield.clone().into_iter().collect(),
                        ))
                        .await;

                    info!("{} connected to our server", ip);
                    let _ = peer_writer_handle.sender.send(Message::Unchoke).await;
                    let _ = sender
                        .send(Action {
                            id: ip.clone(),
                            message: PeerManagerAction::AddPeer(peer_writer_handle),
                        })
                        .await;
                    //_ = interval.tick() => {}
                }
            }
        });
    }
}
