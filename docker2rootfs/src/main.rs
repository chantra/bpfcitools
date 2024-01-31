use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use futures::future::try_join_all;
use tracing::{debug, info};

mod render;

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

    /// Caching directory
    #[arg(short, long)]
    cache: Option<PathBuf>,

    /// Output directory
    #[arg(short, long)]
    output: PathBuf,
}

async fn download_layer(
    client: &dkregistry::v2::Client,
    image: &str,
    layer_digest: &str,
    cache_dir: Option<&PathBuf>,
) -> Result<Vec<u8>> {
    if cache_dir.is_some() {
        let cache_path = cache_dir.unwrap().join(layer_digest);
        if cache_path.exists() {
            debug!("Using cached layer {}", layer_digest);
            return fs::read(cache_path)
                .context(format!("Failed to read cached layer {}", layer_digest));
        }
    }
    let blob = client.get_blob(image, layer_digest).await?;
    if cache_dir.is_some() {
        let cache_path = cache_dir.unwrap().join(layer_digest);
        debug!("Caching layer {}", layer_digest);
        fs::write(cache_path, &blob).context(format!("Failed to cache layer {}", layer_digest))?;
    }
    Ok(blob)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

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

    info!("Downloading {} layer(s)", layers_digests.len());

    let blob_futures = layers_digests
        .iter()
        .map(|layer_digest| {
            download_layer(&dclient, &args.image, layer_digest, args.cache.as_ref())
        })
        .collect::<Vec<_>>();
    let blobs = try_join_all(blob_futures).await?;

    info!("Downloaded {} layer(s)", blobs.len());

    info!("Unpacking layers to {}", args.output.display());

    let canonical_path = args.output.canonicalize().unwrap();
    render::unpack(&blobs, &canonical_path)?;
    Ok(())
}
