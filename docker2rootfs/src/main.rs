use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use futures::future::try_join_all;
use tracing::{debug, info, trace};

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

    /// The architecture of the image to download. Some images are built
    /// for multiple architectures. When this happens, this option *must* be
    /// provided to know what image to download.
    /// The variants are listed in https://github.com/opencontainers/image-spec/blob/main/image-index.md#platform-variants
    #[arg(short, long)]
    architecture: Option<String>,

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

/// Returns the manifest for any image:reference.
/// If the image is an OCI image index, returns the underlying image matching the requested architecture.
/// Otherwise, return the Image manifest originally returned by the API.
/// Errors if this is an OCI image list and no architecture flag was passed, or if no image manifest matches the architecture.
async fn get_manifest(
    client: &dkregistry::v2::Client,
    args: &Args,
) -> Result<dkregistry::v2::manifest::Manifest> {
    let mut manifest = client.get_manifest(&args.image, &args.reference).await?;
    match manifest {
        dkregistry::v2::manifest::Manifest::ML(ml) => {
            debug!("Got an Image List, finding matching platform.");
            trace!("Image List: {:?}", ml);
            if args.architecture.is_none() {
                return Err(anyhow::anyhow!("This image is availale for multiple architectures, you need to specify which one you want with --architecture flag."));
            }
            for item in ml.manifests {
                if Some(item.architecture()) == args.architecture {
                    manifest = client.get_manifest(&args.image, &item.digest).await?;
                    return Ok(manifest);
                }
            }
            Err(anyhow::anyhow!(
                "No manifest found for architecture {}",
                args.architecture.as_ref().unwrap()
            ))
        }
        _ => Ok(manifest),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

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
    let manifest = get_manifest(&dclient, &args).await.unwrap_or_else(|_| {
        panic!(
            "Did not find a manifest for {}:{}",
            args.image, args.reference
        )
    });

    trace!("Manifest: {:?}", manifest);
    let layers_digests = manifest.layers_digests(None)?;

    trace!("Layers digests: {:?}", layers_digests);
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
