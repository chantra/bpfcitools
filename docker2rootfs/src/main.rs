use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use futures::future::try_join_all;
use oci_distribution::{
    manifest::{OciImageManifest, OciManifest},
    secrets::RegistryAuth,
    Client, Reference,
};
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

async fn download_layer(client: &Client, args: &Args, layer_digest: &str) -> Result<Vec<u8>> {
    if args.cache.is_some() {
        let cache_path = args.cache.as_ref().unwrap().join(layer_digest);
        if cache_path.exists() {
            debug!("Using cached layer {}", layer_digest);
            return fs::read(cache_path)
                .context(format!("Failed to read cached layer {}", layer_digest));
        }
    }
    let reference = Reference::with_digest(
        args.registry.clone(),
        args.image.clone(),
        layer_digest.to_string(),
    );

    let mut blob: Vec<u8> = Vec::new();
    client
        .pull_blob(&reference, layer_digest, &mut blob)
        .await?;
    if args.cache.is_some() {
        let cache_path = args.cache.as_ref().unwrap().join(layer_digest);
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
    client: &mut Client,
    auth: &RegistryAuth,
    args: &Args,
) -> Result<OciImageManifest> {
    // Build a URL which will be accepted by oci-distribution.
    let reference = Reference::with_tag(
        args.registry.clone(),
        args.image.clone(),
        args.reference.clone(),
    );
    let (manifest, _) = client
        .pull_manifest(&reference, auth)
        .await
        .expect("Cannot pull manifest");

    match manifest {
        OciManifest::ImageIndex(ii) => {
            debug!("Got an OCI Image Index, finding matching platform.");
            trace!("Image List: {:?}", ii);
            if args.architecture.is_none() {
                return Err(anyhow::anyhow!("This image is available for multiple architectures, you need to specify which one you want with --architecture flag."));
            }
            for item in ii.manifests {
                if item
                    .platform
                    .is_some_and(|p| Some(p.architecture) == args.architecture)
                {
                    let reference = Reference::with_digest(
                        args.registry.clone(),
                        args.image.clone(),
                        item.digest,
                    );

                    let (manifest, _) = client
                        .pull_manifest(&reference, auth)
                        .await
                        .expect("Cannot pull manifest");
                    match manifest {
                        OciManifest::Image(manifest) => return Ok(manifest),
                        _ => unreachable!(),
                    }
                }
            }
            Err(anyhow::anyhow!(
                "No manifest found for architecture {}",
                args.architecture.as_ref().unwrap()
            ))
        }
        OciManifest::Image(manifest) => Ok(manifest),
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

    let auth = RegistryAuth::Anonymous;
    let mut client = Client::new(oci_distribution::client::ClientConfig {
        ..Default::default()
    });

    let manifest = get_manifest(&mut client, &auth, &args).await?;

    trace!("Manifest: {:?}", manifest);

    let layers_digests: Vec<_> = manifest
        .layers
        .iter()
        .map(|layer| layer.digest.clone())
        .collect();

    trace!("Layers digests: {:?}", layers_digests);
    info!("Downloading {} layer(s)", layers_digests.len());

    let blob_futures = layers_digests
        .iter()
        .map(|layer_digest| download_layer(&client, &args, layer_digest))
        .collect::<Vec<_>>();
    let blobs = try_join_all(blob_futures).await?;

    info!("Downloaded {} layer(s)", blobs.len());

    info!("Unpacking layers to {}", args.output.display());

    let canonical_path = args.output.canonicalize().unwrap();
    render::unpack(&blobs, &canonical_path)?;

    Ok(())
}
