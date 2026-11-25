mod i18n;

use anyhow::{Context, Result, bail, ensure};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{ACCEPT, AUTHORIZATION, LINK};
use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

const OWNER: &str = "wxmyyds";
const REGISTRY: &str = "https://ghcr.io";
const CATALOG_URL: &str = "https://raw.githubusercontent.com/wxmyyds/arch-cloud-pkgs/main/cpkg/packages.json";
const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json";
const PKGINFO_ANNOTATION: &str = "org.wxmyyds.cpkg.pkginfo";
const MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
const ARTIFACT_MEDIA_TYPE: &str = "application/vnd.wxmyyds.cpkg.v1";
const CONFIG_MEDIA_TYPE: &str = "application/vnd.oci.empty.v1+json";
const LAYER_MEDIA_TYPE: &str = "application/vnd.wxmyyds.cpkg.pkg.v1";

fn client_version() -> &'static str {
    option_env!("CPKG_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

macro_rules! tr {
    ($key:ident) => {
        i18n::text(i18n::Key::$key)
    };
    ($key:ident, $($name:ident = $value:expr),+ $(,)?) => {
        i18n::format(i18n::Key::$key, &[$((stringify!($name), format!("{}", $value))),+])
    };
}

#[derive(Debug)]
enum PacmanAction {
    List,
    Info(Vec<String>),
    Search(String),
    Check(Vec<String>),
    Install(Vec<(String, Option<String>)>, Vec<OsString>),
    Upgrade(Vec<OsString>),
}

#[derive(Debug, Deserialize)]
struct Catalog {
    schema_version: u32,
    packages: Vec<CatalogPackage>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogPackage {
    name: String,
    description: String,
    #[serde(default)]
    depends: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Descriptor {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mediaType")]
enum OciDocument {
    #[serde(rename = "application/vnd.oci.image.manifest.v1+json")]
    Manifest(Manifest),
    #[serde(rename = "application/vnd.oci.image.index.v1+json")]
    Index(ImageIndex),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "artifactType")]
    artifact_type: String,
    config: Descriptor,
    annotations: HashMap<String, String>,
    layers: Vec<Descriptor>,
}

#[derive(Debug, Deserialize)]
struct ImageIndex {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    manifests: Vec<IndexDescriptor>,
}

#[derive(Debug, Deserialize)]
struct IndexDescriptor {
    #[serde(flatten)]
    descriptor: Descriptor,
    platform: Platform,
}

#[derive(Debug, Deserialize)]
struct Platform {
    os: String,
    architecture: String,
    variant: Option<String>,
}

#[derive(Debug, Clone)]
struct PackageMetadata {
    name: String,
    version: String,
    architecture: String,
    description: String,
    pkginfo: String,
    layer: Layer,
}

#[derive(Debug, Clone)]
struct Layer {
    digest: String,
    size: u64,
}

#[derive(Debug)]
struct TransactionPackage {
    metadata: PackageMetadata,
    current: Option<String>,
    /// Named on the cpkg command line rather than pulled in as a cloud dependency.
    target: bool,
}

impl TransactionPackage {
    /// A package cpkg pulled in only as a cloud dependency and that is not
    /// installed yet. pacman records every `-U` target as explicitly installed,
    /// so these must be installed with `--asdeps` unless the caller asked for a
    /// specific reason; otherwise `pacman -Rns <package>` leaves them behind.
    fn fresh_dependency(&self) -> bool {
        !self.target && self.current.is_none()
    }
}

struct RegistryClient {
    http: HttpClient,
    repository: String,
    token: String,
}

impl Catalog {
    fn load(http: &HttpClient) -> Result<Self> {
        let catalog = http
            .get(CATALOG_URL)
            .send()?
            .error_for_status()?
            .json::<Self>()
            .context(tr!(CatalogParseFailed))?;
        ensure!(catalog.schema_version == 1, "{}", tr!(CatalogVersionUnsupported, version = catalog.schema_version));
        ensure!(!catalog.packages.is_empty(), "{}", tr!(CatalogEmpty));
        for package in &catalog.packages {
            ensure!(valid_package_name(&package.name), "{}", tr!(CatalogInvalidPackageName, name = package.name));
        }
        Ok(catalog)
    }

    fn find(&self, name: &str) -> Result<&CatalogPackage> {
        self.packages
            .iter()
            .find(|package| package.name == name)
            .with_context(|| tr!(PackageNotFound, name = name))
    }
}

impl RegistryClient {
    fn new(http: HttpClient, package: &CatalogPackage) -> Result<Self> {
        let repository = format!("{OWNER}/{}", package.name);
        let scope = format!("repository:{repository}:pull");
        let response = http
            .get(format!("{REGISTRY}/token"))
            .query(&[("service", "ghcr.io"), ("scope", scope.as_str())])
            .send()?
            .error_for_status()?
            .json::<TokenResponse>()
            .context(tr!(GhcrTokenFailed))?;
        Ok(Self { http, repository, token: response.token })
    }

    fn auth(&self, request: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        request.header(AUTHORIZATION, format!("Bearer {}", self.token))
    }

    fn tags(&self) -> Result<Vec<String>> {
        let first = format!("{REGISTRY}/v2/{}/tags/list?n=1000", self.repository);
        let mut next = Some(first);
        let mut visited = HashSet::new();
        let mut tags = Vec::new();
        while let Some(url) = next.take() {
            ensure!(visited.insert(url.clone()), "{}", tr!(GhcrRepeatedPagination));
            let response = self.auth(self.http.get(&url)).send()?.error_for_status()?;
            let link = response
                .headers()
                .get(LINK)
                .map(|value| value.to_str().context(tr!(GhcrUnexpectedPagination)))
                .transpose()?
                .map(str::to_owned);
            let page: TagsResponse = response.json().context(tr!(GhcrTagsFailed))?;
            tags.extend(page.tags);
            next = match link.as_deref().and_then(next_link) {
                Some(link) if link.starts_with("/v2/") => Some(format!("{REGISTRY}{link}")),
                Some(link) if link.starts_with("https://ghcr.io/v2/") => Some(link.to_owned()),
                Some(_) => bail!("{}", tr!(GhcrUnexpectedPagination)),
                None => None,
            };
        }
        Ok(tags)
    }

    fn latest_version(&self) -> Result<String> {
        let mut versions = self.tags()?.into_iter().filter_map(decode_tag);
        let mut latest = versions.next().context(tr!(GhcrNoVersions))?;
        for candidate in versions {
            if vercmp(&candidate, &latest)? > 0 {
                latest = candidate;
            }
        }
        Ok(latest)
    }

    fn metadata(&self, version: &str, expected_name: &str) -> Result<PackageMetadata> {
        let tag = encode_tag(version)?;
        let url = format!("{REGISTRY}/v2/{}/manifests/{tag}", self.repository);
        let response = self.auth(self.http.get(url)).header(ACCEPT, MANIFEST_ACCEPT).send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("{}", tr!(GhcrVersionNotFound, version = version));
        }
        let document = response.error_for_status()?.bytes()?;
        resolve_metadata(&document, version, expected_name, std::env::consts::OS, std::env::consts::ARCH, |descriptor| {
            let url = format!("{REGISTRY}/v2/{}/manifests/{}", self.repository, descriptor.digest);
            let response = self.auth(self.http.get(url)).header(ACCEPT, MANIFEST_MEDIA_TYPE).send()?.error_for_status()?;
            Ok(response.bytes()?.to_vec())
        })
    }

    fn download(&self, metadata: &PackageMetadata, directory: &Path) -> Result<PathBuf> {
        let url = format!("{REGISTRY}/v2/{}/blobs/{}", self.repository, metadata.layer.digest);
        let response = self.auth(self.http.get(url)).send()?.error_for_status()?;
        let digest_hex = metadata.layer.digest.strip_prefix("sha256:").context(tr!(DownloadDigestMalformed))?;
        let filename = format!("{}-{digest_hex}.pkg.tar.zst", metadata.name);
        let path = directory.join(filename);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .context(tr!(DownloadFileCreateFailed))?;
        let mut response = response;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 128 * 1024];
        let mut copied = 0u64;
        let progress = ProgressBar::new(metadata.layer.size);
        progress.set_style(ProgressStyle::with_template(
            "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}",
        )?);
        progress.set_message(metadata.name.clone());
        loop {
            let count = response.read(&mut buffer).context(tr!(DownloadBlobReadFailed))?;
            if count == 0 { break; }
            file.write_all(&buffer[..count])?;
            hasher.update(&buffer[..count]);
            copied += count as u64;
            progress.set_position(copied);
        }
        file.flush()?;
        progress.finish_and_clear();
        ensure!(copied == metadata.layer.size, "{}", tr!(DownloadSizeMismatch, expected = metadata.layer.size, actual = copied));
        let actual_digest = format!("sha256:{:x}", hasher.finalize());
        ensure!(actual_digest == metadata.layer.digest, "{}", tr!(DownloadDigestMismatch));
        Ok(path)
    }
}

fn native_oci_architecture(os: &str, arch: &str) -> Result<&'static str> {
    ensure!(os == "linux", "{}", tr!(OciPlatformUnsupported, os = os, architecture = arch));
    match arch {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        _ => bail!("{}", tr!(OciPlatformUnsupported, os = os, architecture = arch)),
    }
}

fn validate_descriptor(descriptor: &Descriptor, media_type: &str) -> Result<()> {
    ensure!(descriptor.media_type == media_type && descriptor.size > 0, "{}", tr!(OciDescriptorInvalid));
    validate_digest(&descriptor.digest)
}

impl ImageIndex {
    fn select(&self, os: &str, arch: &str) -> Result<&Descriptor> {
        let architecture = native_oci_architecture(os, arch)?;
        ensure!(self.schema_version == 2 && !self.manifests.is_empty(), "{}", tr!(OciIndexInvalid));
        let mut platforms = HashSet::new();
        let mut selected = None;
        // Validate every entry before selecting: a broken index must never become a flat-manifest request.
        for entry in &self.manifests {
            validate_descriptor(&entry.descriptor, MANIFEST_MEDIA_TYPE)?;
            let platform = &entry.platform;
            ensure!(platform.os == "linux" && matches!(platform.architecture.as_str(), "amd64" | "arm64"), "{}", tr!(OciIndexInvalid));
            ensure!(platform.variant.is_none() || (platform.architecture == "arm64" && platform.variant.as_deref() == Some("v8")), "{}", tr!(OciIndexInvalid));
            ensure!(platforms.insert(platform.architecture.as_str()), "{}", tr!(OciIndexAmbiguous));
            if platform.architecture == architecture {
                selected = Some(&entry.descriptor);
            }
        }
        selected.with_context(|| tr!(OciIndexPlatformMissing, architecture = architecture))
    }
}

fn resolve_metadata(
    document: &[u8],
    version: &str,
    expected_name: &str,
    os: &str,
    arch: &str,
    mut fetch_manifest: impl FnMut(&Descriptor) -> Result<Vec<u8>>,
) -> Result<PackageMetadata> {
    native_oci_architecture(os, arch)?;
    let document: OciDocument = serde_json::from_slice(document).context(tr!(OciManifestFailed))?;
    let (manifest, indexed) = match document {
        OciDocument::Manifest(manifest) => (manifest, false),
        OciDocument::Index(index) => {
            let descriptor = index.select(os, arch)?;
            let bytes = fetch_manifest(descriptor)?;
            ensure!(bytes.len() as u64 == descriptor.size, "{}", tr!(OciChildSizeMismatch));
            let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
            ensure!(digest == descriptor.digest, "{}", tr!(OciChildDigestMismatch));
            match serde_json::from_slice(&bytes).context(tr!(OciManifestFailed))? {
                OciDocument::Manifest(manifest) => (manifest, true),
                OciDocument::Index(_) => bail!("{}", tr!(OciDescriptorInvalid)),
            }
        }
    };
    ensure!(manifest.schema_version == 2 && manifest.artifact_type == ARTIFACT_MEDIA_TYPE, "{}", tr!(OciManifestInvalid));
    validate_descriptor(&manifest.config, CONFIG_MEDIA_TYPE)?;
    ensure!(manifest.config.size == 2 && manifest.config.digest == format!("sha256:{:x}", Sha256::digest(b"{}")), "{}", tr!(OciManifestInvalid));
    let pkginfo = manifest.annotations.get(PKGINFO_ANNOTATION).context(tr!(OciPkginfoMissing))?.clone();
    let name = pkginfo_field(&pkginfo, "pkgname").context(tr!(PkgNameMissing))?;
    let version_from_pkginfo = pkginfo_field(&pkginfo, "pkgver").context(tr!(PkgVersionMissing))?;
    let architecture = pkginfo_field(&pkginfo, "arch").context(tr!(PkgArchitectureMissing))?;
    let description = pkginfo_field(&pkginfo, "pkgdesc").context(tr!(PkgDescriptionMissing))?;
    ensure!(name == expected_name, "{}", tr!(PkgNameMismatch, expected = expected_name, actual = name));
    ensure!(version_from_pkginfo == version, "{}", tr!(PkgVersionMismatch, tag = version, metadata = version_from_pkginfo));
    // Only flat single-file publications can be architecture-independent.
    ensure!(architecture == arch || (!indexed && architecture == "any"), "{}", tr!(PkgArchitectureMismatch, architecture = architecture));
    ensure!(manifest.layers.len() == 1, "{}", tr!(OciLayerCount));
    let descriptor = &manifest.layers[0];
    ensure!(descriptor.media_type == LAYER_MEDIA_TYPE, "{}", tr!(OciLayerType));
    validate_descriptor(descriptor, LAYER_MEDIA_TYPE)?;
    Ok(PackageMetadata {
        name,
        version: version.to_owned(),
        architecture,
        description,
        pkginfo,
        layer: Layer { digest: descriptor.digest.clone(), size: descriptor.size },
    })
}

fn next_link(header: &str) -> Option<&str> {
    header.split(',').find_map(|part| {
        if !part.contains("rel=\"next\"") && !part.contains("rel=next") {
            return None;
        }
        let start = part.find('<')? + 1;
        let end = part[start..].find('>')? + start;
        Some(part[start..end].trim())
    })
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn encode_tag(version: &str) -> Result<String> {
    ensure!(!version.is_empty(), "{}", tr!(VersionEmpty));
    let mut tag = String::from("v");
    for byte in version.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-' {
            tag.push(byte as char);
        } else {
            tag.push('_');
            tag.push_str(&format!("{byte:02X}"));
        }
    }
    ensure!(tag.len() <= 128, "{}", tr!(VersionTagTooLong));
    Ok(tag)
}

fn decode_tag(tag: String) -> Option<String> {
    let encoded = tag.strip_prefix('v')?;
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'_' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn pkginfo_field(pkginfo: &str, key: &str) -> Option<String> {
    pkginfo.lines().find_map(|line| {
        let (field, value) = line.split_once(" = ")?;
        (field == key).then(|| value.to_owned())
    })
}

fn validate_digest(digest: &str) -> Result<()> {
    let hex = digest.strip_prefix("sha256:").context(tr!(DigestUnsupported))?;
    ensure!(hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "{}", tr!(DigestMalformed));
    Ok(())
}

fn vercmp(left: &str, right: &str) -> Result<i8> {
    let output = Command::new("vercmp").arg(left).arg(right).output().context(tr!(VercmpRunFailed))?;
    ensure!(output.status.success(), "{}", tr!(VercmpCompareFailed));
    let value = String::from_utf8_lossy(&output.stdout).trim().parse::<i8>()?;
    ensure!((-1..=1).contains(&value), "{}", tr!(VercmpUnexpectedOutput, value = value));
    Ok(value)
}

fn installed_version(package: &str) -> Result<Option<String>> {
    let output = Command::new("pacman")
        .args(["-Q", package])
        .env("LC_ALL", "C")
        .output()
        .context(tr!(PacmanQueryFailed))?;
    if output.status.success() {
        let stdout = String::from_utf8(output.stdout).context(tr!(PacmanQueryOutputInvalid))?;
        let version = stdout
            .split_whitespace()
            .nth(1)
            .context(tr!(PacmanQueryOutputInvalid))?;
        return Ok(Some(version.to_owned()));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() == Some(1)
        && stderr.contains(&format!("package '{package}' was not found"))
    {
        return Ok(None);
    }
    bail!("{}: {}", tr!(PacmanQueryFailed), stderr.trim())
}

fn is_root() -> Result<bool> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context(tr!(IdentityCheckFailed))?;
    ensure!(output.status.success(), "{}", tr!(IdentityCheckFailed));
    let uid = String::from_utf8(output.stdout)
        .context(tr!(IdentityCheckFailed))?
        .trim()
        .parse::<u32>()
        .context(tr!(IdentityCheckFailed))?;
    Ok(uid == 0)
}

fn is_sync_sysupgrade(args: &[OsString]) -> bool {
    let values = args.iter().filter_map(|arg| arg.to_str());
    let (mut sync, mut sysupgrade) = (false, false);
    for value in values {
        if value == "--" { break; }
        match value {
            "--sync" => sync = true,
            "--sysupgrade" => sysupgrade = true,
            _ if value.starts_with('-') && !value.starts_with("--") => {
                sync |= value[1..].contains('S');
                sysupgrade |= value[1..].contains('u');
            }
            _ => {}
        }
    }
    sync && sysupgrade
}

fn cloud_upgrade_args(args: &[OsString]) -> Vec<OsString> {
    let mut result = Vec::new();
    for arg in args {
        let Some(value) = arg.to_str() else {
            result.push(arg.clone());
            continue;
        };
        match value {
            "--sync" | "--refresh" | "--sysupgrade" => continue,
            _ if value.starts_with('-') && !value.starts_with("--") => {
                let remaining: String = value[1..].chars().filter(|flag| !matches!(flag, 'S' | 'y' | 'u')).collect();
                if !remaining.is_empty() {
                    result.push(OsString::from(format!("-{remaining}")));
                }
            }
            _ => result.push(arg.clone()),
        }
    }
    result
}

fn parse_search_args(args: &[OsString]) -> Result<Option<String>> {
    let values = args.iter().map(|arg| arg.to_str()).collect::<Option<Vec<_>>>();
    let Some(values) = values else { return Ok(None); };
    if values.is_empty() { return Ok(None); }

    let query = match values[0] {
        "-Ss" | "-sS" => {
            ensure!(values.len() == 2, "{}", tr!(SearchShortUsage));
            Some(values[1])
        }
        "-S" | "--sync" if values.len() >= 2 && matches!(values[1], "-s" | "--search") => {
            ensure!(values.len() == 3, "{}", tr!(SearchLongUsage));
            Some(values[2])
        }
        "-S" | "--sync" if values.len() == 2 && values[1].starts_with("--search=") => {
            Some(values[1].trim_start_matches("--search="))
        }
        _ => None,
    };
    Ok(query.map(str::to_owned))
}

fn search_matches(package: &CatalogPackage, pattern: &Regex) -> bool {
    pattern.is_match(&package.name) || pattern.is_match(&package.description)
}

fn search_packages(http: &HttpClient, catalog: &Catalog, query: &str) -> Result<()> {
    let pattern = RegexBuilder::new(query).case_insensitive(true).build().context(tr!(SearchPatternInvalid))?;
    let matches: Vec<&CatalogPackage> = catalog.packages.iter().filter(|package| search_matches(package, &pattern)).collect();
    if matches.is_empty() {
        println!("{}", tr!(SearchNoMatches, query = query));
        return Ok(());
    }
    for package in matches {
        let version = registry_for(http, catalog, &package.name)?.latest_version()?;
        println!("ghcr.io/{OWNER}/{} {}", package.name, version);
        println!("{}: {}", tr!(SearchDescription), package.description);
    }
    Ok(())
}

fn is_sync_list(args: &[OsString]) -> bool {
    args.first().and_then(|arg| arg.to_str()).is_some_and(|first| {
        first == "-Sl" || (first == "--sync" && args.get(1).and_then(|arg| arg.to_str()) == Some("--list"))
    })
}

fn is_sync_info(args: &[OsString]) -> bool {
    args.first().and_then(|arg| arg.to_str()).is_some_and(|first| {
        first == "-Si" || (first == "--sync" && args.get(1).and_then(|arg| arg.to_str()) == Some("--info"))
    })
}

fn parse_info_targets(args: &[OsString]) -> Result<Vec<String>> {
    let start = if args.first().and_then(|arg| arg.to_str()) == Some("--sync") { 2 } else { 1 };
    let targets = args[start..].iter()
        .map(|arg| arg.to_str().context(tr!(InvalidUtf8PackageName)).map(str::to_owned))
        .collect::<Result<Vec<_>>>()?;
    ensure!(!targets.is_empty(), "{}", tr!(InfoUsage));
    Ok(targets)
}

fn is_query_upgrades(args: &[OsString]) -> bool {
    args.first().and_then(|arg| arg.to_str()).is_some_and(|first| {
        first == "-Qu" || (first == "--query" && args.get(1).and_then(|arg| arg.to_str()) == Some("--upgrades"))
    })
}

fn pacman_option_takes_value(option: &str) -> bool {
    matches!(option,
        "--arch" | "--ask" | "--assume-installed" | "--cachedir" | "--color" | "--config"
        | "--dbpath" | "--gpgdir" | "--hookdir" | "--ignore" | "--ignoregroup"
        | "--logfile" | "--overwrite" | "--print-format" | "--root" | "--sysroot"
        | "-b" | "-r")
}

fn parse_install_args(args: &[OsString]) -> Result<(Vec<(String, Option<String>)>, Vec<OsString>)> {
    let first = args.first().and_then(|arg| arg.to_str()).context(tr!(OperationMissing))?;
    let mut pacman_args = Vec::new();
    let mut targets = Vec::new();
    let mut options_ended = false;
    let mut expect_option_value = false;

    if first == "--sync" {
        for arg in &args[1..] {
            let value = arg.to_str();
            if expect_option_value {
                pacman_args.push(arg.clone());
                expect_option_value = false;
                continue;
            }
            let Some(value) = value else {
                pacman_args.push(arg.clone());
                continue;
            };
            if matches!(value, "--refresh" | "-y") { continue; }
            if value == "--" { options_ended = true; continue; }
            if !options_ended && value.starts_with('-') {
                expect_option_value = pacman_option_takes_value(value);
                pacman_args.push(arg.clone());
            } else {
                targets.push(parse_package_target(value)?);
            }
        }
    } else {
        let remainder: String = first.strip_prefix('-').unwrap_or(first).chars().filter(|flag| !matches!(flag, 'S' | 'y')).collect();
        if !remainder.is_empty() { pacman_args.push(OsString::from(format!("-{remainder}"))); }
        for arg in &args[1..] {
            let value = arg.to_str();
            if expect_option_value {
                pacman_args.push(arg.clone());
                expect_option_value = false;
                continue;
            }
            let Some(value) = value else {
                pacman_args.push(arg.clone());
                continue;
            };
            if matches!(value, "--refresh" | "-y") { continue; }
            if value == "--" { options_ended = true; continue; }
            if !options_ended && value.starts_with('-') {
                expect_option_value = pacman_option_takes_value(value);
                pacman_args.push(arg.clone());
            } else {
                targets.push(parse_package_target(value)?);
            }
        }
    }
    ensure!(!expect_option_value, "{}", tr!(OptionValueMissing));
    ensure!(!targets.is_empty(), "{}", tr!(InstallUsage));
    Ok((targets, pacman_args))
}

fn parse_package_target(target: &str) -> Result<(String, Option<String>)> {
    let (name, version) = match target.split_once('=') {
        Some((name, version)) => {
            ensure!(!version.is_empty(), "{}", tr!(InstallVersionEmpty, target = target));
            (name, Some(version.to_owned()))
        }
        None => (target, None),
    };
    ensure!(valid_package_name(name), "{}", tr!(InstallTargetInvalid, target = target));
    Ok((name.to_owned(), version))
}

fn pacman_action(args: &[OsString]) -> Result<PacmanAction> {
    let first = args.first().and_then(|arg| arg.to_str()).context(tr!(OperationMissing))?;
    if let Some(query) = parse_search_args(args)? {
        return Ok(PacmanAction::Search(query));
    }
    if is_sync_list(args) { return Ok(PacmanAction::List); }
    if is_sync_info(args) { return Ok(PacmanAction::Info(parse_info_targets(args)?)); }
    if is_query_upgrades(args) {
        let start = if first == "--query" { 2 } else { 1 };
        let packages = args[start..].iter()
            .map(|arg| arg.to_str().context(tr!(InvalidUtf8PackageName)).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        return Ok(PacmanAction::Check(packages));
    }
    if is_sync_sysupgrade(args) {
        return Ok(PacmanAction::Upgrade(cloud_upgrade_args(args)));
    }
    if first == "-S" || first == "--sync" || (first.starts_with("-S") && !first.starts_with("--")) {
        let (targets, pacman_args) = parse_install_args(args)?;
        return Ok(PacmanAction::Install(targets, pacman_args));
    }
    if first.starts_with('-') {
        bail!("{}", tr!(LocalOperationUnsupported))
    }
    bail!("{}", tr!(CloudOperationUnsupported))
}

fn registry_for(http: &HttpClient, catalog: &Catalog, name: &str) -> Result<RegistryClient> {
    RegistryClient::new(http.clone(), catalog.find(name)?)
}

fn print_help() {
    println!("{}", tr!(HelpTitle, version = client_version()));
    println!("\n{}", tr!(HelpOperations));
    println!("{}", tr!(HelpList));
    println!("{}", tr!(HelpInfo));
    println!("{}", tr!(HelpSearch));
    println!("{}", tr!(HelpCheck));
    println!("{}", tr!(HelpInstall));
    println!("{}", tr!(HelpUpgrade));
    println!("\n{}", tr!(HelpLocalOperations));
}

fn run() -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.is_empty() || args.first().and_then(|arg| arg.to_str()) == Some("--help") {
        print_help();
        return Ok(());
    }
    if args.first().and_then(|arg| arg.to_str()) == Some("--version") {
        println!("{}", tr!(VersionTitle, version = client_version()));
        return Ok(());
    }

    let action = pacman_action(&args)?;
    let http = HttpClient::builder().user_agent(format!("cpkg/{}", client_version())).build()?;
    let catalog = Catalog::load(&http)?;
    match action {
        PacmanAction::List => {
            println!("{}", tr!(ListHeader));
            for package in &catalog.packages {
                let version = registry_for(&http, &catalog, &package.name)?.latest_version()?;
                println!("{:<20} {:<24} {}", package.name, version, package.description);
            }
        }
        PacmanAction::Info(packages) => {
            for package in packages {
                let registry = registry_for(&http, &catalog, &package)?;
                let selected = registry.latest_version()?;
                let metadata = registry.metadata(&selected, &package)?;
                println!("{}", tr!(InfoName, name = metadata.name));
                println!("{}", tr!(InfoVersion, version = metadata.version));
                println!("{}", tr!(InfoArchitecture, architecture = metadata.architecture));
                println!("{}", tr!(InfoDescription, description = metadata.description));
                println!("{}", tr!(InfoLayer, size = metadata.layer.size, digest = metadata.layer.digest));
                println!("\n{}", metadata.pkginfo);
            }
        }
        PacmanAction::Search(query) => search_packages(&http, &catalog, &query)?,
        PacmanAction::Check(packages) => check_updates(&http, &catalog, &packages)?,
        PacmanAction::Install(targets, pacman_args) => install_many(&http, &catalog, &targets, &pacman_args)?,
        PacmanAction::Upgrade(pacman_args) => upgrade(&http, &catalog, &pacman_args)?,
    }
    Ok(())
}

fn check_updates(http: &HttpClient, catalog: &Catalog, names: &[String]) -> Result<()> {
    let packages: Vec<&CatalogPackage> = if names.is_empty() {
        catalog.packages.iter().collect()
    } else {
        names.iter().map(|name| catalog.find(name)).collect::<Result<Vec<_>>>()?
    };
    let mut updates = 0;
    for package in packages {
        let Some(current) = installed_version(&package.name)? else { continue; };
        let registry = registry_for(http, catalog, &package.name)?;
        let latest = registry.latest_version()?;
        match vercmp(&latest, &current)? {
            1 => { println!("{}", tr!(UpdateAvailable, name = package.name, current = current, latest = latest)); updates += 1; }
            0 => println!("{}", tr!(UpdateCurrent, name = package.name, version = current)),
            _ => println!("{}", tr!(UpdateLocalNewer, name = package.name, current = current, latest = latest)),
        }
    }
    if updates == 0 { println!("{}", tr!(UpdateNone)); }
    Ok(())
}


fn dependency_order(catalog: &Catalog, roots: &[String]) -> Result<Vec<String>> {
    fn visit(
        catalog: &Catalog,
        package: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(package) { return Ok(()); }
        ensure!(visiting.insert(package.to_owned()), "{}", tr!(DependencyCycle, package = package));
        let metadata = catalog.find(package)?;
        for dependency in &metadata.depends {
            visit(catalog, dependency, visiting, visited, ordered)?;
        }
        visiting.remove(package);
        visited.insert(package.to_owned());
        ordered.push(package.to_owned());
        Ok(())
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut ordered = Vec::new();
    for root in roots {
        visit(catalog, root, &mut visiting, &mut visited, &mut ordered)?;
    }
    Ok(ordered)
}

fn needs_cloud_confirmation(pacman_args: &[OsString]) -> bool {
    !pacman_args.iter().any(|arg| arg == "--noconfirm")
}

fn format_size_mib(bytes: u64) -> String {
    format!("{:.2}", bytes as f64 / (1024.0 * 1024.0))
}

fn confirmation_accepts(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "" | "y" | "yes" | "是" | "确认")
}

fn confirm_cloud_transaction(packages: &[TransactionPackage], pacman_args: &[OsString]) -> Result<bool> {
    if !needs_cloud_confirmation(pacman_args) { return Ok(true); }

    println!(":: {}", tr!(TransactionHeader, count = packages.len()));
    let name_width = packages.iter().map(|package| package.metadata.name.len()).max().unwrap_or(0);
    for package in packages {
        let name = &package.metadata.name;
        let target = match &package.current {
            Some(current) => format!("{current} -> {}", package.metadata.version),
            None => package.metadata.version.clone(),
        };
        println!(" {name:<name_width$} {target}");
    }
    let download_size = packages.iter().map(|package| package.metadata.layer.size).sum();
    println!("\n{}", tr!(TransactionDownloadSize, size = format_size_mib(download_size)));
    print!(":: {}{}", tr!(ConfirmTitle), tr!(ConfirmPrompt));
    std::io::stdout().flush()?;

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if confirmation_accepts(&answer) {
        return Ok(true);
    }
    println!("{}", tr!(ConfirmCancelled));
    Ok(false)
}

fn install_many(
    http: &HttpClient,
    catalog: &Catalog,
    targets: &[(String, Option<String>)],
    pacman_args: &[OsString],
) -> Result<()> {
    ensure!(!targets.is_empty(), "{}", tr!(InstallNoPackages));
    let requested_versions: HashMap<String, Option<String>> = targets.iter().cloned().collect();
    let roots: Vec<String> = targets.iter().map(|(name, _)| name.clone()).collect();
    let requested_names: HashSet<&str> = roots.iter().map(String::as_str).collect();
    let order = dependency_order(catalog, &roots)?;
    let mut selected_packages = Vec::new();
    for package in order {
        let registry = registry_for(http, catalog, &package)?;
        let latest = registry.latest_version()?;
        let requested = requested_versions.get(&package).and_then(|version| version.as_deref());
        let selected = requested.unwrap_or(&latest);
        let current = installed_version(&package)?;
        if requested.is_none() {
            if let Some(current) = &current {
                if vercmp(selected, current)? <= 0 {
                    println!("{}", tr!(InstallAlreadyCurrent, name = package, current = current, latest = selected));
                    continue;
                }
            }
        }
        let metadata = registry.metadata(selected, &package)?;
        let target = requested_names.contains(package.as_str());
        selected_packages.push(TransactionPackage { metadata, current, target });
    }
    execute_transaction(http, catalog, &selected_packages, pacman_args)
}

fn upgrade(http: &HttpClient, catalog: &Catalog, pacman_args: &[OsString]) -> Result<()> {
    let mut candidates = Vec::new();
    for package in &catalog.packages {
        let Some(current) = installed_version(&package.name)? else { continue; };
        let registry = registry_for(http, catalog, &package.name)?;
        let latest = registry.latest_version()?;
        if vercmp(&latest, &current)? > 0 {
            let metadata = registry.metadata(&latest, &package.name)?;
            candidates.push(TransactionPackage { metadata, current: Some(current), target: true });
        }
    }
    if candidates.is_empty() {
        println!("{}", tr!(UpdateNone));
        return Ok(());
    }
    execute_transaction(http, catalog, &candidates, pacman_args)
}

fn execute_transaction(
    http: &HttpClient,
    catalog: &Catalog,
    packages: &[TransactionPackage],
    pacman_args: &[OsString],
) -> Result<()> {
    if packages.is_empty() {
        return Ok(());
    }
    if !confirm_cloud_transaction(packages, pacman_args)? { return Ok(()); }
    let temporary = tempdir()?;
    let mut dependencies = Vec::new();
    let mut install_paths = Vec::new();
    for package in packages {
        let registry = registry_for(http, catalog, &package.metadata.name)?;
        let path = registry.download(&package.metadata, temporary.path())?;
        if package.fresh_dependency() {
            dependencies.push(path);
        } else {
            install_paths.push(path);
        }
    }

    // Install fresh cloud dependencies in their own `--asdeps` transaction so
    // pacman records them as dependencies; see TransactionPackage::fresh_dependency.
    // A caller-supplied reason flag keeps everything in one transaction instead.
    if !dependencies.is_empty() && !reason_flag_requested(pacman_args) {
        run_pacman_upgrade(&dependencies, &dependency_install_args(pacman_args))?;
    } else if !dependencies.is_empty() {
        install_paths.extend(dependencies);
    }
    if install_paths.is_empty() {
        return Ok(());
    }
    run_pacman_upgrade(&install_paths, pacman_args)
}

fn reason_flag_requested(pacman_args: &[OsString]) -> bool {
    pacman_args.iter().any(|arg| matches!(arg.to_str(), Some("--asdeps") | Some("--asexplicit")))
}

fn dependency_install_args(pacman_args: &[OsString]) -> Vec<OsString> {
    let mut args = pacman_args.to_vec();
    args.push(OsString::from("--asdeps"));
    args
}

fn run_pacman_upgrade(paths: &[PathBuf], pacman_args: &[OsString]) -> Result<()> {
    ensure!(!paths.is_empty(), "{}", tr!(TransactionNoPackages));
    let root = is_root()?;
    let mut command = if root { Command::new("pacman") } else { Command::new("sudo") };
    if !root { command.arg("pacman"); }
    command.arg("-U").args(pacman_args).args(paths);
    ensure!(command.status()?.success(), "{}", tr!(TransactionFailed));
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{}: {error:#}", tr!(ErrorPrefix));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_fixture(arch: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": MANIFEST_MEDIA_TYPE,
            "artifactType": ARTIFACT_MEDIA_TYPE,
            "config": {
                "mediaType": CONFIG_MEDIA_TYPE,
                "digest": format!("sha256:{:x}", Sha256::digest(b"{}")),
                "size": 2
            },
            "annotations": {
                "org.wxmyyds.cpkg.pkginfo": format!("pkgname = cirrus\npkgver = 1:2.0-3\narch = {arch}\npkgdesc = Cirrus Music\n")
            },
            "layers": [{"mediaType": LAYER_MEDIA_TYPE, "digest": format!("sha256:{}", "a".repeat(64)), "size": 100}]
        })).unwrap()
    }

    fn index_entry(architecture: &str, leaf: &[u8]) -> serde_json::Value {
        serde_json::json!({
            "mediaType": MANIFEST_MEDIA_TYPE,
            "digest": format!("sha256:{:x}", Sha256::digest(leaf)),
            "size": leaf.len(),
            "platform": {"os": "linux", "architecture": architecture}
        })
    }

    fn index_fixture(entries: Vec<serde_json::Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": entries
        })).unwrap()
    }

    #[test]
    fn index_resolves_only_the_requested_native_leaf_by_digest() {
        let x86 = manifest_fixture("x86_64");
        let arm = manifest_fixture("aarch64");
        let entries = vec![index_entry("amd64", &x86), index_entry("arm64", &arm)];
        let document = index_fixture(entries.clone());
        for (arch, leaf, entry) in [("x86_64", &x86, &entries[0]), ("aarch64", &arm, &entries[1])] {
            let mut requests = 0;
            let metadata = resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", arch, |descriptor| {
                requests += 1;
                assert_eq!(descriptor.digest, entry["digest"].as_str().unwrap());
                assert_eq!(descriptor.size, leaf.len() as u64);
                Ok(leaf.clone())
            }).unwrap();
            assert_eq!(requests, 1);
            assert_eq!(metadata.architecture, arch);
            assert_eq!(metadata.name, "cirrus");
        }
    }

    #[test]
    fn index_missing_native_architecture_never_fetches_another_architecture() {
        let document = index_fixture(vec![index_entry("amd64", &manifest_fixture("x86_64"))]);
        assert!(resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", "aarch64", |_| {
            panic!("missing native platform must fail before fetching a leaf")
        }).is_err());
    }

    #[test]
    fn index_rejects_wrong_unknown_or_missing_platforms() {
        let entry = index_entry("arm64", &manifest_fixture("aarch64"));
        let mut wrong_os = entry.clone();
        wrong_os["platform"]["os"] = "windows".into();
        let mut unknown_arch = entry.clone();
        unknown_arch["platform"]["architecture"] = "unknown".into();
        let mut any_arch = entry.clone();
        any_arch["platform"]["architecture"] = "any".into();
        let mut missing = entry.clone();
        missing.as_object_mut().unwrap().remove("platform");
        let mut wrong_variant = entry;
        wrong_variant["platform"]["variant"] = "v7".into();
        for invalid in [wrong_os, unknown_arch, any_arch, missing, wrong_variant] {
            let document = index_fixture(vec![invalid]);
            assert!(resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", "aarch64", |_| {
                panic!("invalid platform must fail before fetching")
            }).is_err());
        }
    }

    #[test]
    fn index_rejects_ambiguous_platforms_including_arm64_variants() {
        let entry = index_entry("arm64", &manifest_fixture("aarch64"));
        let mut variant = entry.clone();
        variant["platform"]["variant"] = "v8".into();
        for duplicate in [entry.clone(), variant] {
            let document = index_fixture(vec![entry.clone(), duplicate]);
            assert!(resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", "aarch64", |_| {
                panic!("ambiguous platform must fail before fetching")
            }).is_err());
        }
    }

    #[test]
    fn index_validates_all_descriptors_and_never_decodes_a_malformed_index_as_flat() {
        let entry = index_entry("amd64", &manifest_fixture("x86_64"));
        let mut wrong_type = entry.clone();
        wrong_type["mediaType"] = LAYER_MEDIA_TYPE.into();
        let mut bad_digest = entry.clone();
        bad_digest["digest"] = "sha256:bad".into();
        let mut empty = entry.clone();
        empty["size"] = 0.into();
        let mut missing_size = entry.clone();
        missing_size.as_object_mut().unwrap().remove("size");
        let mut invalid_documents = vec![index_fixture(vec![])];
        for invalid in [wrong_type, bad_digest, empty, missing_size] {
            // The requested valid entry cannot hide a broken descriptor for another platform.
            invalid_documents.push(index_fixture(vec![index_entry("arm64", &manifest_fixture("aarch64")), invalid]));
        }
        let mut schema: serde_json::Value = serde_json::from_slice(&index_fixture(vec![entry])).unwrap();
        schema["schemaVersion"] = 1.into();
        invalid_documents.push(serde_json::to_vec(&schema).unwrap());
        schema.as_object_mut().unwrap().remove("manifests");
        schema["layers"] = serde_json::json!([]);
        invalid_documents.push(serde_json::to_vec(&schema).unwrap());
        for document in invalid_documents {
            assert!(resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", "aarch64", |_| {
                panic!("malformed index must fail before fetching")
            }).is_err());
        }
    }

    #[test]
    fn index_verifies_fetched_manifest_size_and_digest_before_decoding() {
        let leaf = manifest_fixture("aarch64");
        let document = index_fixture(vec![index_entry("arm64", &leaf)]);
        let mut same_size_wrong_digest = leaf.clone();
        same_size_wrong_digest[0] = b' ';
        for invalid in [Vec::new(), same_size_wrong_digest] {
            assert!(resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", "aarch64", |_| Ok(invalid.clone())).is_err());
        }
    }

    #[test]
    fn index_rejects_leaf_media_type_nested_index_and_architecture_mismatch() {
        let mut wrong_type: serde_json::Value = serde_json::from_slice(&manifest_fixture("aarch64")).unwrap();
        wrong_type["mediaType"] = "application/json".into();
        for leaf in [
            manifest_fixture("x86_64"),
            manifest_fixture("any"),
            index_fixture(vec![index_entry("arm64", &manifest_fixture("aarch64"))]),
            serde_json::to_vec(&wrong_type).unwrap(),
        ] {
            let document = index_fixture(vec![index_entry("arm64", &leaf)]);
            assert!(resolve_metadata(&document, "1:2.0-3", "cirrus", "linux", "aarch64", |_| Ok(leaf.clone())).is_err());
        }
    }

    #[test]
    fn flat_manifests_preserve_single_file_and_architecture_independent_packages() {
        for (host, package) in [("x86_64", "x86_64"), ("aarch64", "aarch64"), ("x86_64", "any"), ("aarch64", "any")] {
            let metadata = resolve_metadata(&manifest_fixture(package), "1:2.0-3", "cirrus", "linux", host, |_| {
                panic!("flat manifest must not fetch a child")
            }).unwrap();
            assert_eq!(metadata.architecture, package);
        }
        assert!(resolve_metadata(&manifest_fixture("x86_64"), "1:2.0-3", "cirrus", "linux", "aarch64", |_| unreachable!()).is_err());
        assert!(resolve_metadata(&manifest_fixture("aarch64"), "1:2.0-3", "wrong-name", "linux", "aarch64", |_| unreachable!()).is_err());
        assert!(resolve_metadata(&manifest_fixture("aarch64"), "2.0-3", "cirrus", "linux", "aarch64", |_| unreachable!()).is_err());
    }

    #[test]
    fn flat_manifests_reject_invalid_schema_config_and_layers() {
        let valid: serde_json::Value = serde_json::from_slice(&manifest_fixture("x86_64")).unwrap();
        for (field, value) in [
            ("schemaVersion", serde_json::json!(1)),
            ("mediaType", serde_json::json!("application/json")),
            ("artifactType", serde_json::json!("unknown")),
            ("config", serde_json::json!({"mediaType": CONFIG_MEDIA_TYPE, "digest": format!("sha256:{}", "a".repeat(64)), "size": 2})),
            ("layers", serde_json::json!([])),
            ("layers", serde_json::json!([{"mediaType": LAYER_MEDIA_TYPE, "digest": "sha256:bad", "size": 100}])),
            ("layers", serde_json::json!([{"mediaType": LAYER_MEDIA_TYPE, "digest": format!("sha256:{}", "a".repeat(64)), "size": 0}])),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(resolve_metadata(&serde_json::to_vec(&invalid).unwrap(), "1:2.0-3", "cirrus", "linux", "x86_64", |_| unreachable!()).is_err());
        }
    }

    #[test]
    fn client_platform_support_is_explicit() {
        assert_eq!(native_oci_architecture("linux", "x86_64").unwrap(), "amd64");
        assert_eq!(native_oci_architecture("linux", "aarch64").unwrap(), "arm64");
        for (os, arch) in [("windows", "x86_64"), ("linux", "riscv64"), ("unknown", "unknown")] {
            assert!(resolve_metadata(&manifest_fixture("any"), "1:2.0-3", "cirrus", os, arch, |_| unreachable!()).is_err());
        }
    }

    #[test]
    fn pacman_style_confirmation_defaults_to_yes() {
        assert!(confirmation_accepts(""));
        assert!(confirmation_accepts("y"));
        assert!(confirmation_accepts("yes"));
        assert!(!confirmation_accepts("n"));
        assert!(!confirmation_accepts("no"));
    }

    #[test]
    fn only_explicit_noconfirm_skips_cloud_confirmation() {
        assert!(needs_cloud_confirmation(&[]));
        assert!(!needs_cloud_confirmation(&[OsString::from("--noconfirm")]));
        assert!(needs_cloud_confirmation(&[OsString::from("--color=always")]));
    }

    #[test]
    fn pacman_sysupgrade_syntax_selects_cloud_upgrade() {
        for args in [
            vec!["-Syu"],
            vec!["-Syyu", "--noconfirm"],
            vec!["--sync", "--refresh", "--sysupgrade"],
        ] {
            let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert!(is_sync_sysupgrade(&args));
        }
        assert!(!is_sync_sysupgrade(&[OsString::from("-Ss"), OsString::from("clavis")]));
    }

    #[test]
    fn cloud_upgrade_removes_sync_operation_flags_only() {
        let args = [OsString::from("-Syu"), OsString::from("--noconfirm")];
        assert_eq!(cloud_upgrade_args(&args), vec![OsString::from("--noconfirm")]);
        let args = [OsString::from("--sync"), OsString::from("--refresh"), OsString::from("--sysupgrade"), OsString::from("--color=always")];
        assert_eq!(cloud_upgrade_args(&args), vec![OsString::from("--color=always")]);
    }

    #[test]
    fn pacman_style_search_options_extract_pattern() {
        for args in [
            vec!["-Ss", "clavis"],
            vec!["--sync", "--search", "clavis"],
            vec!["-S", "--search=clavis"],
        ] {
            let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert_eq!(parse_search_args(&args).unwrap().as_deref(), Some("clavis"));
        }
    }

    #[test]
    fn pacman_style_info_action_accepts_targets() {
        let args = [OsString::from("-Si"), OsString::from("clavis-shell")];
        assert!(matches!(pacman_action(&args).unwrap(), PacmanAction::Info(names) if names == vec!["clavis-shell".to_owned()]));
    }

    #[test]
    fn pacman_operation_flags_select_cloud_actions() {
        let install = [OsString::from("-S"), OsString::from("clavis-shell")];
        assert!(matches!(pacman_action(&install).unwrap(), PacmanAction::Install(_, _)));
        let upgrade = [OsString::from("-Syu"), OsString::from("--noconfirm")];
        assert!(matches!(pacman_action(&upgrade).unwrap(), PacmanAction::Upgrade(args) if args == vec![OsString::from("--noconfirm")]));
        let check = [OsString::from("-Qu")];
        assert!(matches!(pacman_action(&check).unwrap(), PacmanAction::Check(names) if names.is_empty()));
    }

    #[test]
    fn non_cloud_commands_are_rejected() {
        let subcommand = [OsString::from("install"), OsString::from("clavis-shell")];
        assert!(pacman_action(&subcommand).is_err());
        let removal = [OsString::from("-Rns"), OsString::from("key-cli")];
        assert!(pacman_action(&removal).is_err());
    }

    #[test]
    fn search_matches_package_name_and_description_case_insensitively() {
        let package = CatalogPackage {
            name: "clavis-shell".to_owned(),
            description: "Clavis qml/Qt6 shell".to_owned(),
            depends: Vec::new(),
        };
        let pattern = RegexBuilder::new("CLAVIS").case_insensitive(true).build().unwrap();
        assert!(search_matches(&package, &pattern));
        assert!(search_matches(&package, &Regex::new("qml").unwrap()));
        assert!(!search_matches(&package, &Regex::new("wechat").unwrap()));
    }

    #[test]
    fn clavis_dependency_order_includes_all_runtime_packages() {
        let catalog: Catalog = serde_json::from_str(include_str!("../packages.json")).unwrap();
        let order = dependency_order(&catalog, &["clavis-shell".to_owned()]).unwrap();
        assert_eq!(order, vec!["key-cli", "libcava", "qt6-m3shapes-git", "clavis-shell"].into_iter().map(str::to_owned).collect::<Vec<_>>());
    }

    fn transaction_metadata(name: &str) -> PackageMetadata {
        PackageMetadata {
            name: name.to_owned(),
            version: "1.0-1".to_owned(),
            architecture: "x86_64".to_owned(),
            description: String::new(),
            pkginfo: String::new(),
            layer: Layer { digest: String::new(), size: 0 },
        }
    }

    #[test]
    fn only_unrequested_fresh_packages_install_as_dependencies() {
        let fresh_dependency = TransactionPackage { metadata: transaction_metadata("key-cli"), current: None, target: false };
        assert!(fresh_dependency.fresh_dependency());

        let requested = TransactionPackage { metadata: transaction_metadata("clavis-shell"), current: None, target: true };
        assert!(!requested.fresh_dependency());

        // Upgrades keep the reason pacman already recorded, so they stay explicit.
        let installed = TransactionPackage {
            metadata: transaction_metadata("libcava"),
            current: Some("1.0.0-1".to_owned()),
            target: false,
        };
        assert!(!installed.fresh_dependency());
    }

    #[test]
    fn dependency_transaction_forces_deps_unless_caller_picks_a_reason() {
        let args = vec![OsString::from("--noconfirm")];
        assert!(!reason_flag_requested(&args));
        assert_eq!(dependency_install_args(&args), vec![OsString::from("--noconfirm"), OsString::from("--asdeps")]);
        assert!(reason_flag_requested(&[OsString::from("--asdeps")]));
        assert!(reason_flag_requested(&[OsString::from("--asexplicit")]));
        assert!(!reason_flag_requested(&[OsString::from("--color=always")]));
    }

    #[test]
    fn pacman_install_parses_package_equals_version_and_options() {
        let args = [
            OsString::from("-S"),
            OsString::from("clavis-shell=1.2.3-1"),
            OsString::from("key-cli"),
            OsString::from("--noconfirm"),
        ];
        let (targets, forwarded) = parse_install_args(&args).unwrap();
        assert_eq!(targets, vec![
            ("clavis-shell".to_owned(), Some("1.2.3-1".to_owned())),
            ("key-cli".to_owned(), None),
        ]);
        assert_eq!(forwarded, vec![OsString::from("--noconfirm")]);
    }

    #[test]
    fn version_tags_round_trip_pacman_characters() {
        for version in ["2.0.0-1", "1:1.6.10-1", "1.0+r23-2", "1.0_alpha-1"] {
            assert_eq!(decode_tag(encode_tag(version).unwrap()).as_deref(), Some(version));
        }
    }

    #[test]
    fn ignores_non_cpkg_tags() {
        assert_eq!(decode_tag("latest".to_owned()), None);
        assert_eq!(decode_tag("v1_0".to_owned()), None);
    }

    #[test]
    fn parses_pkginfo_fields() {
        let info = "pkgname = demo\npkgver = 1:2.0-3\ndepend = glibc\ndepend = zlib\n";
        assert_eq!(pkginfo_field(info, "pkgname").as_deref(), Some("demo"));
        assert_eq!(pkginfo_field(info, "pkgver").as_deref(), Some("1:2.0-3"));
    }

    #[test]
    fn rejects_invalid_digests() {
        assert!(validate_digest("sha256:xyz").is_err());
        assert!(validate_digest(&format!("sha256:{}", "A".repeat(64))).is_err());
        assert!(validate_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
    }
}
