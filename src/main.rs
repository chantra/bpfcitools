use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use dkregistry::render;
use futures::future::try_join_all;

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

    /// Output directory
    #[arg(short, long)]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Create the directory if it does not exist.
    fs::create_dir_all(&args.output).context(format!(
        "Failed to create output directory {}",
        args.output.display()
    ))?;
    if fs::read_dir(&args.output)
        .context(format!(
            "Failed to read output directory {}",
            args.output.display()
        ))?
        .count()
        > 0
    {
        anyhow::bail!("Output directory {} is not empty", args.output.display());
    }

    let client = dkregistry::v2::Client::configure()
        .registry(&args.registry)
        .insecure_registry(false)
        .build()?;

    let login_scope = format!("repository:{}:pull", args.image);
    let dclient = client.authenticate(&[&login_scope]).await?;
    let manifest = dclient.get_manifest(&args.image, &args.reference).await?;
    let layers_digests = manifest.layers_digests(None)?;

    println!("Downloading {} layer(s)", layers_digests.len());

    let blob_futures = layers_digests
        .iter()
        .map(|layer_digest| dclient.get_blob(&args.image, layer_digest))
        .collect::<Vec<_>>();
    let blobs = try_join_all(blob_futures).await?;

    println!("Downloaded {} layer(s)", blobs.len());

    println!("Unpacking layers to {}", args.output.display());

    render::unpack(&blobs, &args.output)?;
    Ok(())
}
