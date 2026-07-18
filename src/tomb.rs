use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

// Default size for a new tomb.
pub const DEFAULT_TOMB_SIZE: &str = "50m";

// Default volume name for the disk image.
pub const DEFAULT_VOLUME_NAME: &str = "pass-tomb";

// The filename extension for tomb disk images.
pub const TOMB_EXTENSION: &str = ".dmg";

// Options for creating a new tomb.
pub struct CreateOptions {
    // Path where the tomb .dmg file will be created.
    pub tomb_path: PathBuf,
    // Size of the disk image (e.g. "50m", "100m", "1g").
    pub size: String,
    // Volume name (label shown when mounted).
    pub volume_name: String,
    // Filesystem type (default "APFS").
    pub filesystem: String,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            tomb_path: PathBuf::from("pass-tomb.dmg"),
            size: DEFAULT_TOMB_SIZE.to_string(),
            volume_name: DEFAULT_VOLUME_NAME.to_string(),
            filesystem: "APFS".to_string(),
        }
    }
}

pub fn open_tomb(tomb_path: &Path) -> Result<PathBuf> {
    if !tomb_path.exists() {
        bail!("tomb not found: {}", tomb_path.display());
    }

    let output = Command::new("hdiutil")
        .arg("attach")
        .arg("-plist")
        .arg(tomb_path)
        .output()
        .context("failed to run hdiutil attach")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("hdiutil attach failed: {stderr}");
    }

    let mount_point = parse_attach_plist(&output.stdout)?
        .context("hdiutil attached but no mount point found in output")?;

    Ok(mount_point)
}

pub fn open_tomb_with_stdin_pass(
    tomb_path: &Path,
    password: Option<&str>,
    mountpoint: Option<&Path>,
) -> Result<PathBuf> {
    if !tomb_path.exists() {
        bail!("tomb not found: {}", tomb_path.display());
    }

    let mut cmd = Command::new("hdiutil");
    cmd.args(["attach", "-plist", "-stdinpass"]);
    if let Some(mp) = mountpoint {
        std::fs::create_dir_all(mp).ok();
        cmd.arg("-mountpoint");
        cmd.arg(mp);
    }
    cmd.arg(tomb_path);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if password.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }

    let mut child = cmd.spawn().context("failed to spawn hdiutil attach")?;

    if let Some(pass) = password {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(pass.as_bytes())?;
            stdin.write_all(&[0u8])?; // null-terminated
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("hdiutil attach failed: {stderr}");
    }

    let mount_point = parse_attach_plist(&output.stdout)?
        .context("hdiutil attached but no mount point found in output")?;

    Ok(mount_point)
}

// Parse the mount point from `hdiutil attach -plist` output.
fn parse_attach_plist(data: &[u8]) -> Result<Option<PathBuf>> {
    let plist_str = String::from_utf8_lossy(data);

    let key = "<key>mount-point</key>";
    let Some(key_start) = plist_str.find(key) else {
        return Ok(None);
    };

    let after_key = &plist_str[key_start + key.len()..];
    let string_start_tag = "<string>";
    let string_end_tag = "</string>";

    let Some(string_start) = after_key.find(string_start_tag) else {
        return Ok(None);
    };
    let content_start = string_start + string_start_tag.len();
    let Some(content_end) = after_key[content_start..].find(string_end_tag) else {
        return Ok(None);
    };

    let mount_path = &after_key[content_start..content_start + content_end];
    Ok(Some(PathBuf::from(mount_path)))
}

pub fn close_tomb(path: &Path) -> Result<()> {
    let mount_point = if path.extension().map_or(false, |e| e == "dmg") {
        let mp = find_mount_point(path)?.context("tomb is not currently mounted")?;
        mp
    } else {
        path.to_path_buf()
    };

    close_tomb_by_mount(&mount_point)
}

// Close a tomb by its mount point path.
pub fn close_tomb_by_mount(mount_point: &Path) -> Result<()> {
    let output = Command::new("hdiutil")
        .arg("detach")
        .arg(mount_point)
        .stderr(std::process::Stdio::piped())
        .output()
        .context("failed to run hdiutil detach")?;

    if !output.status.success() {
        // hdiutil stderr is intentionally discarded
        bail!(
            "hdiutil detach failed for {} (exit: {:?})",
            mount_point.display(),
            output.status.code()
        );
    }
    Ok(())
}

pub fn close_tomb_by_name(name: &str) -> Result<()> {
    let tombs = list_open_tombs()?;
    for t in &tombs {
        if t.volume_name == name || t.mount_point.to_string_lossy().contains(name) {
            return close_tomb_by_mount(&t.mount_point);
        }
    }

    // Try a direct detach by volume name
    let output = Command::new("hdiutil")
        .arg("detach")
        .arg(format!("/Volumes/{}", name))
        .stderr(std::process::Stdio::null())
        .output()
        .context("failed to run hdiutil detach")?;

    if output.status.success() {
        return Ok(());
    }

    bail!("Could not find an open tomb named '{}' to close.", name);
}

// Information about an open (mounted) tomb.
#[derive(Debug, Clone)]
pub struct TombInfo {
    // The tomb file path (if determinable).
    pub image_path: Option<PathBuf>,
    // The mount point.
    pub mount_point: PathBuf,
    // The volume name.
    pub volume_name: String,
}

// List all currently mounted tombs.
pub fn list_open_tombs() -> Result<Vec<TombInfo>> {
    let output = Command::new("hdiutil")
        .arg("info")
        .arg("-plist")
        .output()
        .context("failed to run hdiutil info")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("hdiutil info failed: {stderr}");
    }

    parse_info_plist(&output.stdout)
}

// Find the mount point for a given tomb file.
pub fn find_mount_point(tomb_path: &Path) -> Result<Option<PathBuf>> {
    let tombs = list_open_tombs()?;
    let canonical = tomb_path.canonicalize().ok();
    Ok(tombs.into_iter().find_map(|t| {
        if t.image_path.as_ref().and_then(|p| p.canonicalize().ok()) == canonical {
            Some(t.mount_point)
        } else {
            None
        }
    }))
}

// Check if a tomb is currently mounted.
#[allow(dead_code)]
pub fn is_mounted(tomb_path: &Path) -> Result<bool> {
    Ok(find_mount_point(tomb_path)?.is_some())
}

// Parse `hdiutil info -plist` output to extract mounted disk image info.
//
// The plist has this structure:
// ```xml
// <array>
//     <dict>
//         <key>image-path</key>
//         <string>/path/to/tomb.dmg</string>
//         <key>system-entities</key>
//         <array>
//             <dict>
//                 <key>mount-point</key>
//                 <string>/Volumes/...</string>
//             </dict>
//         </array>
//     </dict>
// </array>
// ```
// We extract image-path from the outer dict and mount-point from the inner one.

fn parse_info_plist(data: &[u8]) -> Result<Vec<TombInfo>> {
    let plist_str = String::from_utf8_lossy(data);
    let mut results = Vec::new();

    // Find each outer <dict> (the top-level image entries)
    let mut pos = 0;
    while let Some(dict_start) = plist_str[pos..].find("<dict>") {
        let abs_start = pos + dict_start;
        // Find the matching </dict> by counting nesting
        let mut depth = 1;
        let mut search_pos = abs_start + 6;
        while depth > 0 {
            let next_open = plist_str[search_pos..].find("<dict>");
            let next_close = plist_str[search_pos..].find("</dict>");
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    search_pos = search_pos + o + 6;
                }
                (_, Some(c)) => {
                    depth -= 1;
                    search_pos = search_pos + c + 7;
                }
                _ => break,
            }
        }
        let dict_content = &plist_str[abs_start..search_pos];

        // Only process top-level dicts that have an image-path
        let image_path = extract_plist_string(dict_content, "image-path");
        if let Some(ip) = image_path {
            // Look for mount-point inside nested system-entities array
            if let Some(mount_point) = find_nested_mount_point(dict_content) {
                results.push(TombInfo {
                    image_path: Some(PathBuf::from(ip)),
                    mount_point: PathBuf::from(mount_point),
                    volume_name: mount_point
                        .rsplit('/')
                        .next()
                        .unwrap_or("unknown")
                        .to_string(),
                });
            }
        }

        pos = search_pos;
    }

    Ok(results)
}

// Search a plist dict for a mount-point key, handling nested arrays.
fn find_nested_mount_point(dict_xml: &str) -> Option<&str> {
    // First try direct (unlikely in hdiutil output but handle it)
    if let Some(mp) = extract_plist_string(dict_xml, "mount-point") {
        return Some(mp);
    }
    // Search inside nested <array> for a <dict> that has mount-point
    let mut pos = 0;
    while let Some(array_start) = dict_xml[pos..].find("<array>") {
        let abs_start = pos + array_start;
        // Find matching </array>
        let mut depth = 1;
        let mut search_pos = abs_start + 7;
        while depth > 0 {
            let next_open = dict_xml[search_pos..].find("<array>");
            let next_close = dict_xml[search_pos..].find("</array>");
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    search_pos = search_pos + o + 7;
                }
                (_, Some(c)) => {
                    depth -= 1;
                    search_pos = search_pos + c + 8;
                }
                _ => break,
            }
        }
        let array_content = &dict_xml[abs_start..search_pos];
        if let Some(mp) = extract_plist_string(array_content, "mount-point") {
            return Some(mp);
        }
        pos = search_pos;
    }
    None
}

// Extract a string value for a given key from a plist dict snippet.
fn extract_plist_string<'a>(dict_xml: &'a str, key: &str) -> Option<&'a str> {
    let key_pattern = format!("<key>{}</key>", key);
    let key_start = dict_xml.find(&key_pattern)?;
    let after_key = &dict_xml[key_start + key_pattern.len()..];

    let string_start_tag = "<string>";
    let string_end_tag = "</string>";

    let ss = after_key.find(string_start_tag)?;
    let content_start = ss + string_start_tag.len();
    let ce = after_key[content_start..].find(string_end_tag)?;

    Some(&after_key[content_start..content_start + ce])
}

// Change the passphrase on an existing tomb disk image.
#[allow(dead_code)]
pub fn resize_tomb(tomb_path: &Path, new_size: &str, password: &[u8]) -> Result<()> {
    let mut child = Command::new("hdiutil")
        .args(["resize", "-stdinpass", "-size", new_size])
        .arg(tomb_path)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn hdiutil resize")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(password)?;
        stdin.write_all(&[0u8])?;
    }

    let status = child.wait()?;
    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("hdiutil resize failed (exit: {})", code);
    }

    Ok(())
}

pub fn change_passphrase(tomb_path: &Path) -> Result<()> {
    if !tomb_path.exists() {
        bail!("tomb not found: {}", tomb_path.display());
    }

    let status = Command::new("hdiutil")
        .arg("chpass")
        .arg(tomb_path)
        .status()
        .context("failed to run hdiutil chpass")?;

    if !status.success() {
        bail!("hdiutil chpass failed (exit: {:?})", status.code());
    }

    Ok(())
}

pub fn change_tomb_password(tomb_path: &Path, old_pass: &[u8], new_pass: &[u8]) -> Result<()> {
    let mut child = Command::new("hdiutil")
        .args(["chpass", "-oldstdinpass", "-newstdinpass"])
        .arg(tomb_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn hdiutil chpass")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(old_pass)?;
        stdin.write_all(&[0u8])?; // null-terminate old password
        stdin.write_all(new_pass)?;
        stdin.write_all(&[0u8])?; // null-terminate new password
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("hdiutil chpass failed: {stderr}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attach_plist() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>system-entities</key>
    <array>
        <dict>
            <key>content-hint</key>
            <string>GUID_partition_scheme</string>
            <key>image-path</key>
            <string>/Users/test/pass-tomb.dmg</string>
            <key>mount-point</key>
            <string>/Volumes/pass-tomb</string>
        </dict>
    </array>
</dict>
</plist>"#;

        let result = parse_attach_plist(xml.as_bytes()).unwrap();
        assert_eq!(result, Some(PathBuf::from("/Volumes/pass-tomb")));
    }

    #[test]
    fn test_parse_info_plist() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<array>
    <dict>
        <key>agent</key>
        <string>com.apple.DiskArbitrarion.diskarbitrationd</string>
        <key>image-path</key>
        <string>/Users/test/pass-tomb.dmg</string>
        <key>system-entities</key>
        <array>
            <dict>
                <key>mount-point</key>
                <string>/Volumes/pass-tomb</string>
            </dict>
        </array>
    </dict>
</array>
</plist>"#;

        let result = parse_info_plist(xml.as_bytes()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mount_point, PathBuf::from("/Volumes/pass-tomb"));
        assert_eq!(
            result[0].image_path,
            Some(PathBuf::from("/Users/test/pass-tomb.dmg"))
        );
    }
}
