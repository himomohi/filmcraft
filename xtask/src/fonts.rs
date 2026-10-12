//! Reproducible, licensed build inputs for the default web and packaged interfaces.
//! Font bytes stay in a separate craft-fonts checkout beneath the ignored target directory.
//! An explicit CRAFT_FONTS_DIR is preserved in a generated overlay; its files are never edited.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use sha2::{Digest, Sha256};

const CRAFT_REPO: &str = "https://github.com/storytold/craft-fonts";
// This Japanese-only input preserves the existing release faces and contains no Adobe fonts.
const CRAFT_REV: &str = "abb83316d96aa59c1cf64784289e378fe9fa5695";
const NANUM_REV: &str = "bd8f81ddb5c74d5c8897b36ad88b440266245103";
const NANUM_SHA: &str = "76f45ef4a6bcff344c837c95a7dcc26e017e38b5846d5ae0cdcb5b86be2e2d31";
const LICENSE_SHA: &str = "eeacf16032901d0ed0456876ec77b8f0fda6b3fecec7d972f8543eb602e6c30f";
const NANUM_SIZE: usize = 2_054_744;
const LICENSE_SIZE: usize = 4_534;
const MAX_FONT_SIZE: u64 = 25 * 1024 * 1024;

fn git(directory: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "http.lowSpeedLimit=1", "-c", "http.lowSpeedTime=60"])
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .map_err(|e| format!("preparing craft-fonts: {e}"))?;
    if !out.status.success() {
        return Err(format!("preparing craft-fonts: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("craft-fonts metadata is not UTF-8: {e}"))
}

fn verified(bytes: &[u8], size: usize, digest: &str) -> Result<(), String> {
    if bytes.len() != size || format!("{:x}", Sha256::digest(bytes)) != digest {
        return Err("font input does not match its pinned size and SHA-256".into());
    }
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.take(MAX_FONT_SIZE + 1).read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_FONT_SIZE {
        return Err(format!("font input is too large: {}", path.display()));
    }
    Ok(bytes)
}

fn ensure_download(path: &Path, source: &str, size: usize, digest: &str) -> Result<(), String> {
    if path.exists() {
        return verified(&read_bounded(path)?, size, digest).map_err(|e| format!("{}: {e}; remove this corrupt cache file and retry", path.display()));
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
        .build()
        .new_agent();
    let mut response = agent.get(source).call().map_err(|e| format!("downloading the pinned Nanum Gothic build input: {e}"))?;
    let mut bytes = Vec::new();
    response.body_mut().as_reader().take(size.saturating_add(1) as u64).read_to_end(&mut bytes).map_err(|e| format!("reading Nanum Gothic: {e}"))?;
    verified(&bytes, size, digest)?;
    write_changed(path, &bytes)
}

fn write_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn cached_checkout(target: &Path) -> Result<PathBuf, String> {
    let path = target.join("fonts").join(format!("craft-fonts-{CRAFT_REV}"));
    fs::create_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.join(".git").exists() {
        git(&path, &["init", "--quiet"])?;
    }
    if git(&path, &["rev-parse", "HEAD"]).map(|sha| sha.trim() != CRAFT_REV).unwrap_or(true) {
        git(&path, &["fetch", "--depth", "1", CRAFT_REPO, CRAFT_REV])?;
        git(&path, &["checkout", "--quiet", "--detach", CRAFT_REV])?;
    }
    Ok(path)
}

/// Existing explicit font inputs are reused without rewriting their manifest or bytes.
pub fn prepare(target: &Path, explicit: Option<&OsStr>) -> Result<PathBuf, String> {
    let cache = cached_checkout(target)?.canonicalize().map_err(|e| format!("resolving font cache: {e}"))?;
    let source = explicit.map(PathBuf::from).unwrap_or_else(|| cache.clone());
    let source = source.canonicalize().map_err(|e| format!("CRAFT_FONTS_DIR: {e}"))?;
    // Read the default manifest from the pinned git object, not an editable cache copy.
    let manifest = if source == cache {
        git(&cache, &["show", &format!("{CRAFT_REV}:fonts/manifest.txt")])?
    } else {
        fs::read_to_string(source.join("fonts/manifest.txt")).map_err(|e| format!("CRAFT_FONTS_DIR manifest: {e}"))?
    };
    let font = cache.join("fonts/nanum-gothic/NanumGothic-Regular.ttf");
    let license = cache.join("fonts/nanum-gothic/OFL.txt");
    let base = format!("https://raw.githubusercontent.com/google/fonts/{NANUM_REV}/ofl/nanumgothic");
    ensure_download(&font, &format!("{base}/NanumGothic-Regular.ttf"), NANUM_SIZE, NANUM_SHA)?;
    ensure_download(&license, &format!("{base}/OFL.txt"), LICENSE_SIZE, LICENSE_SHA)?;
    let attribution = format!(
        "asset: NanumGothic-Regular.ttf\ntitle: Nanum Gothic Regular\nauthor: NHN Corporation; Sandoll Communications Inc.\nsource: {base}/NanumGothic-Regular.ttf\nlicense: OFL-1.1\nlicense-file: OFL.txt\nadded: 2026-10-12\nnotes: Original, unmodified upstream bytes; pinned by commit and SHA-256.\n"
    );
    write_changed(&font.with_extension("ttf.attribution"), attribution.as_bytes())?;
    let notice = b"Nanum Gothic Regular\nCopyright (c) 2011 NHN Corporation. All rights reserved.\nFont designed by Sandoll Communications Inc.\nLicensed under the SIL Open Font License 1.1; see OFL-nanum-gothic.txt.\n";
    let overlay = target.join("fonts/build-input");
    fs::create_dir_all(overlay.join("fonts")).map_err(|e| format!("creating font build input: {e}"))?;
    let mut output = String::from("# Generated verified build input; font bytes remain in their source checkout.\n");
    for line in manifest.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let mut fields: Vec<String> = line.split(" | ").map(str::to_string).collect();
        let Some(file) = fields.get(2) else { return Err("malformed craft-fonts manifest".into()) };
        if fields.first().is_some_and(|family| family == "NanumGothic") && fields.get(1).is_some_and(|style| style == "Regular") {
            continue;
        }
        let file = source.join(file).canonicalize().map_err(|e| format!("craft-fonts file: {e}"))?;
        if source == cache {
            let digest = fields.get(6).ok_or("missing pinned font SHA-256")?;
            let bytes = read_bounded(&file)?;
            verified(&bytes, bytes.len(), digest)?;
        }
        let file = file.to_str().ok_or("font path is not UTF-8")?;
        if file.contains(" | ") || file.contains(['\n', '\r']) {
            return Err("font path contains a manifest delimiter".into());
        }
        if let Some(slot) = fields.get_mut(2) {
            *slot = file.into();
        }
        output.push_str(&fields.join(" | "));
        output.push('\n');
    }
    output.push_str(&format!(
        "NanumGothic | Regular | {} | Kore,Hang | OFL-1.1 | {} | {NANUM_SHA} | {base}/NanumGothic-Regular.ttf\n",
        font.display(),
        license.display()
    ));
    // Keep licensing next to this generated input for all existing packaging scripts.
    copy_licenses(&source, &overlay)?;
    write_changed(&overlay.join("fonts/nanum-gothic/OFL.txt"), &read_bounded(&license)?)?;
    write_changed(&overlay.join("fonts/nanum-gothic/NOTICE.txt"), notice)?;
    write_changed(&overlay.join("fonts/manifest.txt"), output.as_bytes())?;
    overlay.canonicalize().map_err(|e| format!("resolving font build input: {e}"))
}

/// Include copyright notices and full OFL texts even when CRAFT_FONTS_DIR was initially unset.
pub fn copy_licenses(source: &Path, destination: &Path) -> Result<(), String> {
    let fonts = source.join("fonts");
    for entry in fs::read_dir(&fonts).map_err(|e| format!("font licenses: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let license = entry.path().join("OFL.txt");
        if license.is_file() {
            let family = entry.file_name();
            let dest = if destination.join("fonts").is_dir() {
                destination.join("fonts").join(&family).join("OFL.txt")
            } else {
                destination.join(format!("OFL-{}.txt", family.to_string_lossy()))
            };
            write_changed(&dest, &read_bounded(&license)?)?;
            let notice = entry.path().join("NOTICE.txt");
            if notice.is_file() {
                let dest = if destination.join("fonts").is_dir() {
                    destination.join("fonts").join(&family).join("NOTICE.txt")
                } else {
                    destination.join(format!("NOTICE-{}.txt", family.to_string_lossy()))
                };
                write_changed(&dest, &read_bounded(&notice)?)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_truncated_and_oversized_downloads_are_rejected() {
        let original = b"verified font fixture";
        let sha = format!("{:x}", Sha256::digest(original));
        assert!(verified(original, original.len(), &sha).is_ok());
        assert!(verified(b"modified font fixture", original.len(), &sha).is_err());
        assert!(verified(&original[..4], original.len(), &sha).is_err());
        assert!(verified(original, original.len() - 1, &sha).is_err());
    }
}
