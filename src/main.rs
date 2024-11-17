use client::run;
mod client;
use std::env;

#[tokio::main(worker_threads = 10)]
async fn main() {
    let args: Vec<String> = env::args().collect();
    // TODO generate random peer id
    run(&args[1], 6281, "gaurab".to_string()).await;
}
