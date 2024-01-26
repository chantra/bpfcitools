use anyhow::Result;
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let client = dkregistry::v2::Client::configure()
        .registry(&args.registry)
        .insecure_registry(false)
        .build()?;

    let login_scope = format!("repository:{}:pull", args.image);
    let dclient = client.authenticate(&[&login_scope]).await?;
    let manifest = dclient.get_manifest(&args.image, &args.reference).await?;
    let layers_digests = manifest.layers_digests(None)?;

    for digest in &layers_digests {
        println!("Layer: {:?}", digest);
    }

    Ok(())
}
