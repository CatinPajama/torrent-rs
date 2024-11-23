use clap::Parser;
use client::run;
mod client;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short = 't', help = "Path to your torrent file")]
    torrent_path: String,
    #[arg(short = 'd', help = "Path where you want to download the file")]
    download_path: String,
    #[arg(short = 'p', default_value_t = 6881)]
    port: u32,
}

#[tokio::main(worker_threads = 10)]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    // TODO generate random peer id
    run(&args.torrent_path, &args.download_path, args.port).await;
}
