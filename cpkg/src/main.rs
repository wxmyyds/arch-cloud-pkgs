use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, LINK};
use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

const OWNER: &str = "wxmyyds";
const REGISTRY: &str = "https://ghcr.io";
const CATALOG_URL: &str = "https://raw.githubusercontent.com/wxmyyds/arch-cloud-pkgs/main/cpkg/packages.json";
const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json";
const PKGINFO_ANNOTATION: &str = "org.wxmyyds.cpkg.pkginfo";
const LAYER_MEDIA_TYPE: &str = "application/vnd.wxmyyds.cpkg.pkg.v1";

#[derive(Parser)]
#[command(
    name = "cpkg",
    version,
    about = "Cloud package client with pacman argument passthrough",
    after_help = "Cloud search: cpkg -Ss <pattern> (matches package name and description)",
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "ls")]
    List { package: Option<String> },
    Check { package: Option<String> },
    Install {
        packages: Vec<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(last = true)]
        pacman_args: Vec<OsString>,
    },
    Upgrade {
        #[arg(last = true)]
        pacman_args: Vec<OsString>,
    },
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
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Descriptor {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    annotations: std::collections::HashMap<String, String>,
    layers: Vec<Descriptor>,
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

struct RegistryClient {
    http: HttpClient,
    repository: String,
    token: String,
}

impl Catalog {
    fn load(http: &HttpClient) -> Result<Self> {
        let embedded: Self = serde_json::from_str(include_str!("../packages.json"))?;
        let fetched = http.get(CATALOG_URL).send().and_then(Response::error_for_status);
        let catalog = match fetched {
            Ok(response) => response.json::<Self>().context("decode package catalog")?,
            Err(_) => embedded,
        };
        ensure!(catalog.schema_version == 1, "unsupported package catalog version");
        ensure!(!catalog.packages.is_empty(), "package catalog is empty");
        for package in &catalog.packages {
            ensure!(valid_package_name(&package.name), "invalid package name in catalog: {}", package.name);
        }
        Ok(catalog)
    }

    fn find(&self, name: &str) -> Result<&CatalogPackage> {
        self.packages
            .iter()
            .find(|package| package.name == name)
            .with_context(|| format!("package '{name}' is not in the cpkg catalog"))
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
            .context("decode GHCR anonymous token")?;
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
            ensure!(visited.insert(url.clone()), "GHCR returned a repeated tags pagination link");
            let response = self.auth(self.http.get(&url)).send()?.error_for_status()?;
            let link = response.headers().get(LINK).and_then(|value| value.to_str().ok()).map(str::to_owned);
            let page: TagsResponse = response.json().context("decode GHCR tag list")?;
            tags.extend(page.tags.unwrap_or_default());
            next = match link.as_deref().and_then(next_link) {
                Some(link) if link.starts_with("/v2/") => Some(format!("{REGISTRY}{link}")),
                Some(link) if link.starts_with("https://ghcr.io/v2/") => Some(link.to_owned()),
                Some(_) => bail!("GHCR returned an unexpected pagination URL"),
                None => None,
            };
        }
        Ok(tags)
    }

    fn latest_version(&self) -> Result<String> {
        let mut versions = self.tags()?.into_iter().filter_map(decode_tag);
        let mut latest = versions.next().context("no published versions found")?;
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
            bail!("version {version} was not found in GHCR");
        }
        let manifest: Manifest = response.error_for_status()?.json().context("decode OCI manifest")?;
        let pkginfo = manifest.annotations.get(PKGINFO_ANNOTATION).context("OCI manifest has no .PKGINFO annotation")?.clone();
        let name = pkginfo_field(&pkginfo, "pkgname").context(".PKGINFO has no pkgname")?;
        let version_from_pkginfo = pkginfo_field(&pkginfo, "pkgver").context(".PKGINFO has no pkgver")?;
        let architecture = pkginfo_field(&pkginfo, "arch").context(".PKGINFO has no arch")?;
        let description = pkginfo_field(&pkginfo, "pkgdesc").unwrap_or_default();
        ensure!(name == expected_name, "manifest package name mismatch: expected {expected_name}, got {name}");
        ensure!(version_from_pkginfo == version, "manifest version mismatch: tag is {version}, .PKGINFO says {version_from_pkginfo}");
        ensure!(architecture == "any" || architecture == std::env::consts::ARCH, "package architecture {architecture} does not match this machine");
        ensure!(manifest.layers.len() == 1, "expected one pacman package layer");
        let descriptor = &manifest.layers[0];
        ensure!(descriptor.media_type == LAYER_MEDIA_TYPE, "unexpected package layer media type");
        validate_digest(&descriptor.digest)?;
        Ok(PackageMetadata {
            name,
            version: version.to_owned(),
            architecture,
            description,
            pkginfo,
            layer: Layer { digest: descriptor.digest.clone(), size: descriptor.size },
        })
    }

    fn download(&self, metadata: &PackageMetadata, directory: &Path) -> Result<PathBuf> {
        let url = format!("{REGISTRY}/v2/{}/blobs/{}", self.repository, metadata.layer.digest);
        let response = self.auth(self.http.get(url)).send()?.error_for_status()?;
        let digest_hex = metadata.layer.digest.strip_prefix("sha256:").context("malformed package digest")?;
        let filename = format!("{}-{digest_hex}.pacman", metadata.name);
        let path = directory.join(filename);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .context("create package download file")?;
        let mut response = response;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 128 * 1024];
        let mut copied = 0u64;
        loop {
            let count = response.read(&mut buffer).context("download package blob")?;
            if count == 0 { break; }
            file.write_all(&buffer[..count])?;
            hasher.update(&buffer[..count]);
            copied += count as u64;
        }
        file.flush()?;
        ensure!(copied == metadata.layer.size, "package size mismatch: expected {}, got {copied}", metadata.layer.size);
        let actual_digest = format!("sha256:{:x}", hasher.finalize());
        ensure!(actual_digest == metadata.layer.digest, "package SHA-256 mismatch");
        Ok(path)
    }
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
    ensure!(!version.is_empty(), "empty package version");
    let mut tag = String::from("v");
    for byte in version.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-' {
            tag.push(byte as char);
        } else {
            tag.push('_');
            tag.push_str(&format!("{byte:02X}"));
        }
    }
    ensure!(tag.len() <= 128, "encoded package version exceeds OCI tag length limit");
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
    let hex = digest.strip_prefix("sha256:").context("only sha256 OCI digests are supported")?;
    ensure!(hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()), "malformed SHA-256 digest");
    Ok(())
}

fn vercmp(left: &str, right: &str) -> Result<i8> {
    let output = Command::new("vercmp").arg(left).arg(right).output().context("run vercmp (install pacman)")?;
    ensure!(output.status.success(), "vercmp rejected a version comparison");
    let value = String::from_utf8_lossy(&output.stdout).trim().parse::<i8>()?;
    ensure!((-1..=1).contains(&value), "unexpected vercmp output: {value}");
    Ok(value)
}

fn installed_version(package: &str) -> Option<String> {
    let output = Command::new("pacman").args(["-Q", package]).output().ok()?;
    if !output.status.success() { return None; }
    String::from_utf8(output.stdout).ok()?.split_whitespace().nth(1).map(str::to_owned)
}

fn is_root() -> bool {
    Command::new("id").arg("-u").output().ok().is_some_and(|output| output.status.success() && output.stdout.as_slice() == b"0\n")
}

fn pacman_command() -> Command {
    if is_root() { Command::new("pacman") } else { Command::new("sudo") }
}

fn run_pacman_passthrough(args: &[OsString]) -> Result<()> {
    let mut command = pacman_command();
    if !is_root() { command.arg("pacman"); }
    command.args(args);
    ensure!(command.status()?.success(), "pacman command failed");
    Ok(())
}

fn parse_search_args(args: &[OsString]) -> Result<Option<String>> {
    let values = args.iter().map(|arg| arg.to_str()).collect::<Option<Vec<_>>>();
    let Some(values) = values else { return Ok(None); };
    if values.is_empty() { return Ok(None); }

    let query = match values[0] {
        "-Ss" | "-sS" => {
            ensure!(values.len() == 2, "usage: cpkg -Ss <pattern>");
            Some(values[1])
        }
        "-S" | "--sync" if values.len() >= 2 && matches!(values[1], "-s" | "--search") => {
            ensure!(values.len() == 3, "usage: cpkg --sync --search <pattern>");
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
    let pattern = RegexBuilder::new(query).case_insensitive(true).build().context("invalid package search pattern")?;
    let matches: Vec<&CatalogPackage> = catalog.packages.iter().filter(|package| search_matches(package, &pattern)).collect();
    if matches.is_empty() {
        println!("No packages found matching '{query}'.");
        return Ok(());
    }
    for package in matches {
        match registry_for(http, catalog, &package.name).and_then(|registry| registry.latest_version()) {
            Ok(version) => {
                println!("ghcr.io/{OWNER}/{} {}", package.name, version);
                println!("    {}", package.description);
            }
            Err(error) => println!("ghcr.io/{:<20} unavailable ({error})", package.name),
        }
    }
    Ok(())
}

fn cloud_shorthand(args: &[OsString]) -> Result<Option<(Vec<String>, Vec<OsString>)>> {
    let Some(first) = args.first().and_then(|arg| arg.to_str()) else { return Ok(None); };
    if first.starts_with('-') || matches!(first, "ls" | "check" | "install" | "upgrade") {
        return Ok(None);
    }
    let separator = args.iter().position(|arg| arg.as_os_str() == OsStr::new("--")).unwrap_or(args.len());
    let packages = args[..separator]
        .iter()
        .map(|arg| arg.to_str().context("package names must be valid UTF-8").map(str::to_owned))
        .collect::<Result<Vec<_>>>()?;
    let pacman_args = if separator < args.len() { args[separator + 1..].to_vec() } else { Vec::new() };
    Ok(Some((packages, pacman_args)))
}

fn catalog_package<'a>(catalog: &'a Catalog, name: &str) -> Result<&'a CatalogPackage> {
    catalog.find(name)
}

fn registry_for(http: &HttpClient, catalog: &Catalog, name: &str) -> Result<RegistryClient> {
    RegistryClient::new(http.clone(), catalog_package(catalog, name)?)
}

fn run() -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().collect();
    let search_query = parse_search_args(&args[1..])?;
    if let Some(first) = args.get(1).and_then(|arg| arg.to_str()) {
        if first.starts_with('-') && search_query.is_none() && !matches!(first, "--help" | "--version") {
            return run_pacman_passthrough(&args[1..]);
        }
    }
    let shorthand = cloud_shorthand(&args[1..])?;
    let cli = if shorthand.is_none() && search_query.is_none() { Some(Cli::try_parse_from(args.clone())?) } else { None };
    let http = HttpClient::builder().user_agent(concat!("cpkg/", env!("CARGO_PKG_VERSION"))).build()?;
    let catalog = Catalog::load(&http)?;
    if let Some(query) = search_query {
        return search_packages(&http, &catalog, &query);
    }
    if let Some((packages, pacman_args)) = shorthand {
        return install_many(&http, &catalog, &packages, None, &pacman_args);
    }
    match cli.context("missing cpkg command")?.command {
        Commands::List { package: Some(package) } => {
            let registry = registry_for(&http, &catalog, &package)?;
            let selected = registry.latest_version()?;
            let metadata = registry.metadata(&selected, &package)?;
            println!("{} {} ({})", metadata.name, metadata.version, metadata.architecture);
            println!("{}", metadata.description);
            println!("OCI layer: {} bytes, {}", metadata.layer.size, metadata.layer.digest);
            println!("\n{}", metadata.pkginfo);
        }
        Commands::List { package: None } => {
            for package in &catalog.packages {
                match registry_for(&http, &catalog, &package.name).and_then(|registry| registry.latest_version()) {
                    Ok(version) => println!("{:<20} {:<24} {}", package.name, version, package.description),
                    Err(error) => println!("{:<20} unavailable ({error})", package.name),
                }
            }
        }
        Commands::Check { package } => {
            let packages: Vec<&CatalogPackage> = match package {
                Some(name) => vec![catalog.find(&name)?],
                None => catalog.packages.iter().collect(),
            };
            let mut updates = 0;
            for package in packages {
                let Some(current) = installed_version(&package.name) else { continue; };
                let registry = registry_for(&http, &catalog, &package.name)?;
                let latest = registry.latest_version()?;
                match vercmp(&latest, &current)? {
                    1 => { println!("{}: {} -> {}", package.name, current, latest); updates += 1; }
                    0 => println!("{}: up to date ({current})", package.name),
                    _ => println!("{}: local {} is newer than cloud {} (no downgrade)", package.name, current, latest),
                }
            }
            if updates == 0 { println!("No updates available."); }
        }
        Commands::Install { packages, version, pacman_args } => {
            ensure!(version.is_none() || packages.len() == 1, "--version can only be used with one package");
            install_many(&http, &catalog, &packages, version.as_deref(), &pacman_args)?;
        }
        Commands::Upgrade { pacman_args } => {
            upgrade(&http, &catalog, &pacman_args)?;
        }
    }
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
        ensure!(visiting.insert(package.to_owned()), "dependency cycle involving {package}");
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

fn install_many(
    http: &HttpClient,
    catalog: &Catalog,
    packages: &[String],
    requested: Option<&str>,
    pacman_args: &[OsString],
) -> Result<()> {
    ensure!(!packages.is_empty(), "specify at least one package to install");
    let order = dependency_order(catalog, packages)?;
    let roots: HashSet<&str> = packages.iter().map(String::as_str).collect();
    let mut selected_packages = Vec::new();
    for package in order {
        let registry = registry_for(http, catalog, &package)?;
        let latest = registry.latest_version()?;
        let selected = if roots.contains(package.as_str()) { requested.unwrap_or(&latest) } else { &latest };
        let current = installed_version(&package);
        if requested.is_none() || !roots.contains(package.as_str()) {
            if let Some(current) = &current {
                if vercmp(selected, current)? <= 0 {
                    println!("{package} is already at {current}; cloud version is {selected}. No downgrade or reinstall.");
                    continue;
                }
            }
        }
        let metadata = registry.metadata(selected, &package)?;
        println!("Install {} {} ({})", metadata.name, metadata.version, metadata.architecture);
        println!("Current: {}", current.as_deref().unwrap_or("not installed"));
        selected_packages.push(metadata);
    }
    if selected_packages.is_empty() { return Ok(()); }
    let temporary = tempdir()?;
    let mut paths = Vec::new();
    for metadata in &selected_packages {
        let registry = registry_for(http, catalog, &metadata.name)?;
        paths.push(registry.download(metadata, temporary.path())?);
    }
    run_pacman_upgrade(&paths, pacman_args)
}

fn upgrade(http: &HttpClient, catalog: &Catalog, pacman_args: &[OsString]) -> Result<()> {
    let mut candidates = Vec::new();
    for package in &catalog.packages {
        let Some(current) = installed_version(&package.name) else { continue; };
        let registry = registry_for(http, catalog, &package.name)?;
        let latest = registry.latest_version()?;
        if vercmp(&latest, &current)? > 0 {
            candidates.push((package.name.clone(), current, latest));
        }
    }
    if candidates.is_empty() { println!("No updates available."); return Ok(()); }
    for (name, current, latest) in &candidates {
        println!("{name}: {current} -> {latest}");
    }
    let temporary = tempdir()?;
    let mut paths = Vec::new();
    for (name, _, version) in &candidates {
        let registry = registry_for(http, catalog, name)?;
        let metadata = registry.metadata(version, name)?;
        paths.push(registry.download(&metadata, temporary.path())?);
    }
    run_pacman_upgrade(&paths, pacman_args)
}

fn run_pacman_upgrade(paths: &[PathBuf], pacman_args: &[OsString]) -> Result<()> {
    ensure!(!paths.is_empty(), "no packages selected");
    let mut command = pacman_command();
    if !is_root() { command.arg("pacman"); }
    command.arg("-U").args(pacman_args).args(paths);
    ensure!(command.status()?.success(), "pacman installation failed");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cpkg: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn ls_accepts_an_optional_package() {
        let cli = Cli::try_parse_from(["cpkg", "ls", "clavis-shell"]).unwrap();
        assert!(matches!(cli.command, Commands::List { package: Some(name) } if name == "clavis-shell"));
    }

    #[test]
    fn search_matches_package_name_and_description_case_insensitively() {
        let package = CatalogPackage {
            name: "clavis-shell".to_owned(),
            description: "Clavis QML/Qt6 shell".to_owned(),
            depends: Vec::new(),
        };
        assert!(search_matches(&package, &Regex::new("CLAVIS").unwrap()));
        assert!(search_matches(&package, &Regex::new("qml").unwrap()));
        assert!(!search_matches(&package, &Regex::new("wechat").unwrap()));
    }

    #[test]
    fn clavis_dependency_order_includes_all_runtime_packages() {
        let catalog: Catalog = serde_json::from_str(include_str!("../packages.json")).unwrap();
        let order = dependency_order(&catalog, &["clavis-shell".to_owned()]).unwrap();
        assert_eq!(order, vec!["key-cli", "libcava", "qt6-m3shapes-git", "clavis-shell"].into_iter().map(str::to_owned).collect::<Vec<_>>());
    }

    #[test]
    fn package_shorthand_preserves_pacman_arguments_after_separator() {
        let args = [
            OsString::from("clavis-shell"),
            OsString::from("--"),
            OsString::from("--noconfirm"),
            OsString::from("--color=always"),
        ];
        let (packages, forwarded) = cloud_shorthand(&args).unwrap().unwrap();
        assert_eq!(packages, vec!["clavis-shell".to_owned()]);
        assert_eq!(
            forwarded,
            vec![OsString::from("--noconfirm"), OsString::from("--color=always")]
        );
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
        assert!(validate_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
    }
}
