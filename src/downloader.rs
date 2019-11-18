use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use console::style;
use reqwest::Client;
use sha1::Sha1;
use tempfile::{NamedTempFile, TempDir};

use crate::pool::ClientPool;
use crate::utils::{fetch_url, spawn_protected, Error};

async fn download_archive(client: &Client, url: String) -> Result<NamedTempFile, Error> {
    let f = NamedTempFile::new()?;
    let body = fetch_url(client, &url).await?;
    fs::write(f.path(), body)?;
    Ok(f)
}

async fn unar(path: &Path) -> Result<TempDir, Error> {
    let dir = TempDir::new()?;
    let status = Command::new("ar")
        .arg("x")
        .arg(path)
        .current_dir(dir.path())
        .status()?;
    if !status.success() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "could not unpack").into());
    }
    Ok(dir)
}

async fn unpack_data(ar_contents: &Path) -> Result<TempDir, Error> {
    let dir = TempDir::new()?;
    for p in fs::read_dir(ar_contents)? {
        let p = p?;
        if let Some("data.tar.xz") | Some("data.tar.gz") | Some("data.tar.bz2") =
            p.file_name().to_str()
        {
            let status = Command::new("gtar")
                .arg("xf")
                .arg(p.path())
                .current_dir(dir.path())
                .status()?;
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "could not unpack").into());
            }
        }
    }
    Ok(dir)
}

async fn sort_images(
    input: &Path,
    output: &Path,
    prefix: &str,
    bundle_id: &str,
) -> Result<(), Error> {
    let _status = Command::new("symsorter")
        .arg("--bundle-id")
        .arg(bundle_id)
        .arg("--prefix")
        .arg(prefix)
        .arg("--output")
        .arg(output)
        .arg("--ignore-errors")
        .arg("-q")
        .arg(input)
        .status()?;
    Ok(())
}

pub async fn download_packages(
    pool: &ClientPool,
    packages: Vec<String>,
    output: &Path,
    prefix: &str,
) -> Result<(), Error> {
    let output = Arc::new(output.to_owned());
    let prefix = Arc::new(prefix.to_owned());
    let download_cache_dir = output.join(prefix.as_str()).join("debscraber_cache");
    fs::create_dir_all(&download_cache_dir)?;

    for package_url in packages {
        // check if the package was already downloaded by checking that the
        // cache file exists.
        let key = Sha1::from(&package_url).digest().to_string();
        let ref_file = download_cache_dir.join(&key);
        if ref_file.is_file() {
            continue;
        }

        let client = pool.get_client().await;
        let output = output.clone();
        let prefix = prefix.clone();
        spawn_protected(async move {
            println!("> {}", style(&package_url).cyan());
            let f = download_archive(&client, package_url).await?;
            let debian_data = unar(&f.path()).await?;
            let archive_contents = unpack_data(debian_data.path()).await?;
            sort_images(archive_contents.path(), &output, &prefix, &key).await?;
            drop(client);
            fs::write(&ref_file, "")?;
            Ok(())
        });
    }

    pool.join().await;

    Ok(())
}
