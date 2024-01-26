use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Registry to pull the image from.
    #[arg(short = 'R', long, default_value = "ghcr.io")]
    registry: String,

    /// Image to download
    #[arg(short, long)]
    image: String,

    /// Reference of the image to download
    #[arg(short, long, default_value = "latest")]
    reference: String,
}

fn main() {
    let args = Args::parse();
    println!("Arguments: {:?}", args);
}
