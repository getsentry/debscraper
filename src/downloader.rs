use std::collections::HashMap;
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
            let status = Command::new("tar")
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
    input: impl Iterator<Item = &Path>,
    output: &Path,
    prefix: &str,
    bundle_suffix: &str,
    bundle_id: &str,
) -> Result<(), Error> {
    let _status = Command::new("symsorter")
        .arg("--bundle-id")
        .arg(&format!("{}-{}", bundle_id, bundle_suffix))
        .arg("--prefix")
        .arg(prefix)
        .arg("--output")
        .arg(output)
        .arg("--ignore-errors")
        .arg("-zz")
        .arg("-q")
        .args(input)
        .status()?;
    Ok(())
}

pub async fn download_packages(
    pool: &ClientPool,
    packages: HashMap<String, Vec<String>>,
    output: &Path,
    prefix: &str,
    bundle_suffix: &str,
) -> Result<(), Error> {
    let output = Arc::new(output.to_owned());
    let prefix = Arc::new(prefix.to_owned());
    let bundle_suffix = Arc::new(bundle_suffix.to_owned());
    let download_cache_dir = Arc::new(output.join(prefix.as_str()).join("debscraber_cache"));
    fs::create_dir_all(&*download_cache_dir)?;

    for (package_name, package_urls) in packages {
        let download_cache_dir = download_cache_dir.clone();
        let client = pool.get_client().await;
        let prefix = prefix.clone();
        let bundle_suffix = bundle_suffix.clone();
        let output = output.clone();
        spawn_protected(async move {
            let mut input_paths = vec![];
            let mut ref_files = vec![];

            for package_url in package_urls {
                // check if the package was already downloaded by checking that the
                // cache file exists.
                let key = Sha1::from(&package_url).digest().to_string();
                let ref_file = download_cache_dir.join(&key);
                if ref_file.is_file() {
                    continue;
                }

                println!("> {}", style(&package_url).cyan());
                let f = download_archive(&client, package_url).await?;
                let debian_data = unar(&f.path()).await?;
                let archive_contents = unpack_data(debian_data.path()).await?;
                input_paths.push(archive_contents);
                ref_files.push(ref_file);
            }

            sort_images(
                input_paths.iter().map(|x| x.path()),
                &output,
                &prefix,
                &bundle_suffix,
                &package_name,
            )
            .await?;

            for ref_file in ref_files {
                fs::write(&ref_file, "")?;
            }

            drop(client);
            Ok(())
        });
    }

    pool.join().await;

    Ok(())
}
