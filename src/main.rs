use client::run;

mod client;

#[tokio::main(worker_threads = 4)]
async fn main() {
    run().await;
}
