use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::tomb;

// The password store directory (default `~/.password-store`).
fn store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        PathBuf::from(dir)
    } else {
        dirs_home().join(".password-store")
    }
}

// The tomb file path (default `~/.password.tomb`).
fn tomb_file() -> PathBuf {
    if let Ok(f) = std::env::var("PASSWORD_STORE_TOMB_FILE") {
        PathBuf::from(f)
    } else {
        dirs_home().join(".password.tomb")
    }
}

// The tomb key file path (default `~/.password.key.tomb`).
fn tomb_key() -> PathBuf {
    if let Ok(f) = std::env::var("PASSWORD_STORE_TOMB_KEY") {
        PathBuf::from(f)
    } else {
        dirs_home().join(".password.key.tomb")
    }
}

// The tomb size in MB (default 30).
fn tomb_size_mb() -> usize {
    if let Ok(s) = std::env::var("PASSWORD_STORE_TOMB_SIZE") {
        s.parse().unwrap_or(30)
    } else {
        30
    }
}

fn dirs_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// The timer file path (default `~/.password.tomb.timer`).
// Lives outside the DMG so it's always accessible.
fn tomb_timer_path() -> PathBuf {
    if let Ok(f) = std::env::var("PASSWORD_STORE_TOMB_TIMER") {
        PathBuf::from(f)
    } else {
        dirs_home().join(".password.tomb.timer")
    }
}

// Ensure the tomb path ends with `.dmg` (hdiutil always creates a `.dmg` file).
fn ensure_dmg(path: &Path) -> PathBuf {
    let s = path.to_string_lossy().to_string();
    if s.ends_with(".dmg") {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("{}.dmg", s))
    }
}

macro_rules! _message { ($($arg:tt)*) => { if !is_quiet() { eprint!("  \x1b[1m.\x1b[0m  "); eprintln!($($arg)*); } } }
macro_rules! _warning { ($($arg:tt)*) => { if !is_quiet() { eprint!("  \x1b[1;33mw\x1b[0m  \x1b[0;33m"); eprintln!($($arg)*); eprint!("\x1b[0m"); } } }
macro_rules! _success { ($($arg:tt)*) => { if !is_quiet() { eprint!(" \x1b[1;32m(*)\x1b[0m \x1b[0;32m"); eprintln!($($arg)*); eprint!("\x1b[0m"); } } }
macro_rules! _verbose { ($($arg:tt)*) => { if is_verbose() { eprint!("  \x1b[1;35m.\x1b[0m  \x1b[0;35mpass\x1b[0m "); eprintln!($($arg)*); } } }
macro_rules! _verbose_tomb { ($($arg:tt)*) => { if is_verbose() { eprint!("  \x1b[1;35m.\x1b[0m  "); eprintln!($($arg)*); } } }

fn is_quiet() -> bool {
    std::env::var("PASSWORD_STORE_QUIET").is_ok_and(|v| v == "1")
}

// Parse a timer string like "30m", "1h", "2d" into seconds.
// Returns None if the format is unrecognized.
fn parse_timer_seconds(timer: &str) -> Option<u64> {
    let timer = timer.trim().to_ascii_lowercase();

    if timer.is_empty() {
        return None;
    }

    let (number, multiplier) = if let Some(n) = timer.strip_suffix("weeks") {
        (n, 7_u64 * 24 * 60 * 60)
    } else if let Some(n) = timer.strip_suffix("week") {
        (n, 7_u64 * 24 * 60 * 60)
    } else if let Some(n) = timer.strip_suffix('w') {
        (n, 7_u64 * 24 * 60 * 60)
    } else if let Some(n) = timer.strip_suffix("days") {
        (n, 24_u64 * 60 * 60)
    } else if let Some(n) = timer.strip_suffix("day") {
        (n, 24_u64 * 60 * 60)
    } else if let Some(n) = timer.strip_suffix('d') {
        (n, 24_u64 * 60 * 60)
    } else if let Some(n) = timer.strip_suffix("hours") {
        (n, 60_u64 * 60)
    } else if let Some(n) = timer.strip_suffix("hour") {
        (n, 60_u64 * 60)
    } else if let Some(n) = timer.strip_suffix('h') {
        (n, 60_u64 * 60)
    } else if let Some(n) = timer.strip_suffix("minutes") {
        (n, 60)
    } else if let Some(n) = timer.strip_suffix("minute") {
        (n, 60)
    } else if let Some(n) = timer.strip_suffix("mins") {
        (n, 60)
    } else if let Some(n) = timer.strip_suffix("min") {
        (n, 60)
    } else if let Some(n) = timer.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = timer.strip_suffix("seconds") {
        (n, 1)
    } else if let Some(n) = timer.strip_suffix("second") {
        (n, 1)
    } else if let Some(n) = timer.strip_suffix("secs") {
        (n, 1)
    } else if let Some(n) = timer.strip_suffix("sec") {
        (n, 1)
    } else if let Some(n) = timer.strip_suffix('s') {
        (n, 1)
    } else {
        (timer.as_str(), 1)
    };

    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    number.parse::<u64>().ok()?.checked_mul(multiplier)
}

fn spawn_auto_close_timer(seconds: u64, mount_point: &Path) -> bool {
    use std::process::Stdio;

    // Pass values as arguments rather than interpolating them into shell code
    // The shell receives seconds as $1 and the mount path as $2
    std::process::Command::new("/usr/bin/nohup")
        .arg("/bin/sh")
        .arg("-c")
        .arg("sleep \"$1\" && exec /usr/bin/hdiutil detach \"$2\"")
        .arg("pass-tomb-auto-close")
        .arg(seconds.to_string())
        .arg(mount_point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

fn is_verbose() -> bool {
    std::env::var("PASSWORD_STORE_VERBOSE").is_ok_and(|v| v == "1")
}

fn get_public_trust(gpg_id: &str) -> Result<String> {
    let output = Command::new("gpg")
        .args(["--with-colons", "--batch", "--list-keys", gpg_id])
        .output()
        .context("Failed to run gpg")?;

    if !output.status.success() {
        bail!("{} is not a valid GPG key ID.", gpg_id);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("pub:") {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() > 1 {
                return Ok(fields[1].to_string());
            }
        }
    }
    bail!("Could not determine trust level for key: {}", gpg_id);
}

// Validate that all GPG IDs are valid and at least one private key is present.
fn validate_gpg_ids(gpg_ids: &[String]) -> Result<()> {
    let trusted = ["m", "f", "u", "w", "s"];

    for id in gpg_ids {
        let trust = get_public_trust(id)?;
        if !trusted.contains(&trust.as_str()) {
            bail!("The key {} is not trusted enough (trust={})", id, trust);
        }
    }

    // At least one private key must be present
    for id in gpg_ids {
        let output = Command::new("gpg")
            .args(["--with-colons", "--batch", "--list-secret-keys", id])
            .output()
            .ok();
        if let Some(out) = output
            && out.status.success()
        {
            return Ok(());
        }
    }

    bail!("You set an invalid GPG ID.");
}

// Encrypt data with GPG for the given recipients.
fn gpg_encrypt(data: &[u8], recipients: &[String], output: &Path) -> Result<()> {
    let mut cmd = Command::new("gpg");
    cmd.args([
        "--batch",
        "--no-tty",
        "--yes",
        "--trust-model",
        "always",
        "--encrypt",
        "--armor",
        "--output",
    ])
    .arg(output)
    .stdin(std::process::Stdio::piped());
    for id in recipients {
        cmd.args(["--recipient", id]);
    }

    let mut child = cmd.spawn().context("Failed to spawn gpg")?;
    child
        .stdin
        .take()
        .context("Failed to open gpg stdin")?
        .write_all(data)
        .context("Failed to write data to gpg")?;

    let status = child.wait()?;
    if !status.success() {
        bail!("gpg encryption failed (exit: {:?})", status.code());
    }
    Ok(())
}

// Decrypt data with GPG.
#[allow(dead_code)]
fn gpg_decrypt(path: &Path) -> Result<Vec<u8>> {
    let output = Command::new("gpg")
        .args(["--decrypt", "--quiet"])
        .arg(path)
        .output()
        .context("Failed to run gpg for decryption")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gpg decryption failed: {stderr}");
    }
    Ok(output.stdout)
}

// Generate a random tomb password (used for the hdiutil disk image).
fn generate_tomb_password() -> String {
    use rand::Rng;
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()-_=+";
    let mut rng = rand::thread_rng();
    (0..64)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

// Set ownership on the mounted volume to the current user.
fn set_ownership(path: &Path) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let status = Command::new("sudo")
        .args(["chown", "-R", &format!("{}:{}", uid, gid)])
        .arg(path)
        .status()
        .context("Failed to run chown")?;

    if !status.success() {
        // Non-fatal
        _verbose!("Could not set ownership (non-fatal)");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_tomb(
    gpg_ids: &[String],
    no_init: bool,
    timer: Option<&str>,
    path: Option<&str>,
    force: bool,
    quiet: bool,
    verbose: bool,
    debug: bool,
    _unsafe_mode: bool,
    size: Option<&str>,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    if let Some(t) = timer {
        match parse_timer_seconds(t) {
            Some(secs) if secs > 0 => {}
            _ => bail!("Invalid timer duration: {}", t),
        }
    }

    if gpg_ids.is_empty() {
        bail!(
            "Usage: {} tomb [-n] [-T time] [-f] [-p subfolder] [-s size] gpg-id...",
            "pass"
        );
    }

    let store_dir_val = store_dir();
    let tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);
    let sz_mb = size
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(tomb_size_mb);
    let subpath = path.unwrap_or("");

    // Set global flags for message helpers
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }
    if verbose {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }
    if debug {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }

    // Sanity checks
    if validate_gpg_ids(gpg_ids).is_err() {
        bail!("You set an invalid GPG ID.");
    }
    if key.exists() {
        bail!(
            "The tomb key {} already exists. I won't overwrite it.",
            key.display()
        );
    }
    if tomb.exists() {
        bail!(
            "The password tomb {} already exists. I won't overwrite it.",
            tomb.display()
        );
    }
    if sz_mb < 10 {
        bail!("A password tomb cannot be smaller than 10 MB.");
    }

    // Forge the tomb key: generate a random password and encrypt it with GPG
    _verbose!(
        "Creating a password tomb with the GPG key(s): {}",
        gpg_ids.join(" ")
    );
    let tomb_pass = generate_tomb_password();
    gpg_encrypt(tomb_pass.as_bytes(), gpg_ids, &key)?;
    if !force && std::env::var("PASSWORD_STORE_TOMB_SIZE").is_err() {
        // Default size
    }

    // Create the encrypted disk image
    _verbose_tomb!("Creating tomb: {} ({} MB)", tomb.display(), sz_mb);
    let create_opts = tomb::CreateOptions {
        tomb_path: tomb.clone(),
        size: format!("{}m", sz_mb),
        volume_name: "pass-tomb".to_string(),
        filesystem: "APFS".to_string(),
    };

    // Pass the tomb password via stdin
    let mut child = Command::new("hdiutil")
        .arg("create")
        .arg("-size")
        .arg(&create_opts.size)
        .arg("-fs")
        .arg(&create_opts.filesystem)
        .arg("-volname")
        .arg(&create_opts.volume_name)
        .arg("-encryption")
        .arg("AES-256")
        .arg("-stdinpass")
        .arg(&create_opts.tomb_path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn hdiutil create")?;

    // Write the tomb password to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(tomb_pass.as_bytes())?;
        stdin.write_all(&[0u8])?;
    }
    let exit = child.wait()?;
    if !exit.success() {
        bail!("hdiutil create failed (exit: {:?})", exit.code());
    }

    _success!("Tomb created: {}", tomb.display());
    _verbose_tomb!("Key forged: {}", key.display());

    // Open the tomb
    let store_path = if subpath.is_empty() {
        store_dir_val.clone()
    } else {
        store_dir_val.join(subpath)
    };
    std::fs::create_dir_all(&store_path)?;

    _verbose!(
        "Opening the password tomb {} using the key {}",
        tomb.display(),
        key.display()
    );
    let mount_point = tomb::open_tomb_with_stdin_pass(&tomb, Some(&tomb_pass), Some(&store_path))?;

    // Wait a moment for the volume to settle
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Set ownership
    set_ownership(&mount_point)?;

    // Initialize the password store using pass init (if available) or manually
    if !no_init {
        // Write .gpg-id (equivalent to `pass init gpg-id...`)
        let gpg_id_content = gpg_ids.join("\n");
        std::fs::write(mount_point.join(".gpg-id"), &gpg_id_content)
            .context("Failed to write .gpg-id")?;

        _success!("Password store initialized for {}", gpg_ids.join(", "));

        // Try to initialize git repository
        let git_init = Command::new("git")
            .args(["init"])
            .current_dir(&mount_point)
            .output()
            .ok();
        if let Some(out) = git_init
            && out.status.success()
        {
            Command::new("git")
                .args(["add", ".gpg-id"])
                .current_dir(&mount_point)
                .output()
                .ok();
            Command::new("git")
                .args(["commit", "-m", "Initialise password store"])
                .current_dir(&mount_point)
                .output()
                .ok();
        }

        // Try `pass init` as well for compatibility
        let _pass_init = Command::new("pass")
            .args(["init"])
            .args(gpg_ids)
            .current_dir(&mount_point)
            .output()
            .ok();
    }

    // Set up timer if requested.
    let mut timed = None;

    let timer_value = if let Some(t) = timer {
        // One-shot timer supplied to this open command.
        Some(t.to_string())
    } else {
        // Persistent timer stored outside the disk image.
        let timer_path = tomb_timer_path();

        if timer_path.exists() {
            let value =
                std::fs::read_to_string(&timer_path).context("failed to read saved timer")?;
            let value = value.trim().to_string();

            if value.is_empty() { None } else { Some(value) }
        } else {
            None
        }
    };

    if let Some(t) = timer_value {
        match parse_timer_seconds(&t) {
            Some(secs) if secs > 0 => {
                if spawn_auto_close_timer(secs, &mount_point) {
                    timed = Some(t);
                } else {
                    eprintln!("Warning: could not start the auto-close timer.");
                }
            }
            _ => {
                eprintln!(
                    "Warning: invalid timer duration '{}'; auto-close was not scheduled.",
                    t
                );
            }
        }
    }

    // Install extension wrappers inside the mount point
    install_internal(false).ok();

    // Success output (matching original)
    _success!(
        "Your password tomb has been created and opened in {}.",
        mount_point.display()
    );
    _message!("Your tomb is: {}", tomb.display());
    _message!("Your tomb key is: {}", key.display());
    if no_init {
        _message!("You need to initialise the store with 'pass init gpg-id...'.");
    } else {
        _message!("You can now use pass as usual.");
    }
    if let Some(t) = timed {
        _message!("This password store will be closed in {}", t);
    } else {
        _message!("When finished, close the password tomb using 'pass close'.");
    }

    Ok(())
}

pub fn cmd_open(
    subfolder: Option<&str>,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
    timer: Option<&str>,
    _force: bool,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    if let Some(t) = timer {
        match parse_timer_seconds(t) {
            Some(secs) if secs > 0 => {}
            _ => bail!("Invalid timer duration: {}", t),
        }
    }
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }
    if verbose {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }

    let path = subfolder.unwrap_or("");
    let tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);

    // Sanity checks
    if !tomb.exists() {
        bail!("There is no password tomb to open: {}", tomb.display());
    }

    // Get the tomb password: try key file first, fall back to prompting
    let tomb_pass = if key.exists() {
        let bytes = gpg_decrypt(&key)?;
        String::from_utf8(bytes)
            .context("Tomb key is not valid UTF-8")?
            .trim()
            .to_string()
    } else {
        _message!("No tomb key found at {}.", key.display());
        _message!("Please enter the tomb password directly.");
        let pass = rpassword::prompt_password("Tomb password: ")
            .context("Failed to read tomb password")?;
        if pass.is_empty() {
            bail!("Password cannot be empty");
        }
        pass
    };

    // Determine the mount point — use PASSWORD_STORE_DIR (or subfolder)
    let store_path = store_dir();
    let mount_target = if path.is_empty() {
        store_path.clone()
    } else {
        store_path.join(path)
    };
    std::fs::create_dir_all(&mount_target)?;

    // Open the tomb at the store path
    _verbose!(
        "Opening the password tomb {} using the key {}",
        tomb.display(),
        key.display()
    );
    let mount_point =
        tomb::open_tomb_with_stdin_pass(&tomb, Some(&tomb_pass), Some(&mount_target))?;

    // Wait for volume to settle
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Set ownership
    set_ownership(&mount_point)?;

    let mut timed = None;

    let timer_value = if let Some(t) = timer {
        Some(t.to_string())
    } else {
        let timer_path = tomb_timer_path();

        if timer_path.exists() {
            let value =
                std::fs::read_to_string(&timer_path).context("failed to read saved timer")?;
            let value = value.trim().to_string();

            if value.is_empty() { None } else { Some(value) }
        } else {
            None
        }
    };

    if let Some(t) = timer_value {
        match parse_timer_seconds(&t) {
            Some(secs) => {
                if spawn_auto_close_timer(secs, &mount_point) {
                    timed = Some(t);
                } else {
                    eprintln!("Warning: could not start the auto-close timer.");
                }
            }
            None => {
                eprintln!(
                    "Warning: invalid timer format '{}'; auto-close was not scheduled.",
                    t
                );
            }
        }
    }

    // Install extension wrappers inside the mount point
    install_internal(false).ok();

    // Success output
    _success!(
        "Your password tomb has been opened in {}.",
        mount_point.display()
    );
    _message!("You can now use pass as usual.");
    if let Some(t) = timed {
        _message!("This password store will be closed in {}", t);
    } else {
        _message!("When finished, close the password tomb using 'pass close'.");
    }

    Ok(())
}

pub fn cmd_close(store_path: Option<&str>, quiet: bool, verbose: bool) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }
    if verbose {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }

    // Determine the tomb file path
    let tomb_file_path = if let Some(p) = store_path {
        let p = PathBuf::from(p);
        if p.extension().is_none_or(|e| e != "dmg") {
            // Might be a store directory
            tomb_file()
        } else {
            p
        }
    } else {
        tomb_file()
    };
    let tomb_file_path = ensure_dmg(&tomb_file_path);

    if !tomb_file_path.exists() {
        // Try closing by default volume name
        tomb::close_tomb_by_name(tomb::DEFAULT_VOLUME_NAME)?;
        _success!("Your password tomb has been closed.");
        return Ok(());
    }

    _verbose!("Closing the password tomb {}", tomb_file_path.display());

    // Try to close by mount point
    let mut closed = false;
    if let Ok(Some(mp)) = tomb::find_mount_point(&tomb_file_path) {
        closed = tomb::close_tomb_by_mount(&mp).is_ok();
    }

    if !closed {
        // Resolve symlinks (e.g. /tmp -> /private/tmp)
        let store = store_dir();
        let store = store.canonicalize().unwrap_or(store);
        if store.exists() {
            closed = tomb::close_tomb_by_mount(&store).is_ok();
        }
    }

    // Final fallback: try by default volume name
    if !closed {
        tomb::close_tomb_by_name(tomb::DEFAULT_VOLUME_NAME)?;
    }

    _success!("Your password tomb has been closed.");
    _message!(
        "Your passwords remain present in {}.",
        tomb_file_path.display()
    );

    Ok(())
}

pub fn cmd_timer(
    timer_value: Option<&str>,
    _store_path: Option<&str>,
    clear: bool,
    quiet: bool,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }

    let timer_path = tomb_timer_path();

    // Clear the persistent timer setting.
    if clear {
        if timer_path.exists() {
            std::fs::remove_file(&timer_path)?;
            _message!("Persistent timer cleared.");
        } else {
            _message!("No persistent timer to clear.");
        }
        return Ok(());
    }

    // Save a persistent timer setting. Do not start a countdown here.
    if let Some(val) = timer_value {
        let secs = parse_timer_seconds(val)
            .ok_or_else(|| anyhow::anyhow!("Invalid timer format: {}", val))?;

        if secs == 0 {
            bail!("Timer duration must be greater than zero.");
        }

        if let Some(parent) = timer_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create timer settings directory")?;
        }

        std::fs::write(&timer_path, val.trim()).context("Failed to save persistent timer")?;

        _message!("Persistent timer set to {}.", val.trim());
        return Ok(());
    }

    // Display the current persistent timer setting.
    if timer_path.exists() {
        let content =
            std::fs::read_to_string(&timer_path).context("Failed to read persistent timer")?;
        let timer = content.trim();

        if timer.is_empty() {
            _message!("No persistent timer set.");
        } else {
            _message!("Persistent timer set: {}", timer);
            println!("{}", timer);
        }
    } else {
        _message!("No persistent timer set.");
    }

    Ok(())
}

// Extract GPG key IDs from an encrypted file using `--status-fd`.
fn gpg_get_recipients(key_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("gpg")
        .args(["--batch", "--status-fd", "1", "--list-only", "--decrypt"])
        .arg(key_path)
        .output()
        .context("Failed to run gpg --list-only")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut keys = Vec::new();
    for line in stdout.lines() {
        if line.starts_with("[GNUPG:] ENC_TO") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                keys.push(parts[2].to_string());
            }
        }
    }

    if keys.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines() {
            if let Some(id) = extract_key_id_from_line(line) {
                keys.push(id);
            }
        }
    }

    if keys.is_empty() {
        bail!("Could not determine GPG recipients from key file");
    }

    keys.sort();
    keys.dedup();
    Ok(keys)
}

// Parse a line like "gpg: encrypted with ... key ID DEADBEEF, ..."
fn extract_key_id_from_line(line: &str) -> Option<String> {
    if line.contains("encrypted with")
        && line.contains("key ID")
        && let Some(idx) = line.find("key ID")
    {
        let rest = &line[idx + 7..];
        let id = rest.trim().split([',', ' ']).next()?;

        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    None
}

// `pass tomb --change` — change the password of an existing password tomb.
pub fn cmd_change(
    quiet: bool,
    verbose: bool,
    debug: bool,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }
    if verbose {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }
    if debug {
        unsafe {
            std::env::set_var("PASSWORD_STORE_DEBUG", "1");
        }
    }

    let tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);

    if !tomb.exists() {
        bail!("Tomb not found: {}", tomb.display());
    }
    if !key.exists() {
        bail!("Tomb key not found: {}", key.display());
    }

    _verbose_tomb!("Key: {}", key.display());
    _verbose_tomb!("Tomb: {}", tomb.display());

    let recipients = gpg_get_recipients(&key)?;
    _verbose!("Current GPG recipients: {}", recipients.join(", "));

    let old_pass_bytes = gpg_decrypt(&key)?;
    let old_pass = String::from_utf8(old_pass_bytes).context("Tomb key is not valid UTF-8")?;
    let old_pass = old_pass.trim().to_string();

    // Prompt for new password
    let new_pass =
        rpassword::prompt_password("New tomb password: ").context("Failed to read new password")?;
    let confirm = rpassword::prompt_password("Confirm new tomb password: ")
        .context("Failed to read password confirmation")?;
    if new_pass != confirm {
        bail!("Passwords do not match");
    }
    if new_pass.is_empty() {
        bail!("Password cannot be empty");
    }

    _message!("Changing password for {}.", tomb.display());
    gpg_encrypt(new_pass.as_bytes(), &recipients, &key)?;
    tomb::change_tomb_password(&tomb, old_pass.as_bytes(), new_pass.as_bytes())?;

    _success!("Password for {} changed.", tomb.display());
    _message!("Your tomb key is: {}", key.display());

    Ok(())
}

// `pass tomb --resize <size>` — resize an existing password tomb.
pub fn cmd_resize(
    new_size: &str,
    quiet: bool,
    verbose: bool,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }
    if verbose {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }

    let tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);

    if !tomb.exists() {
        bail!("Tomb not found: {}", tomb.display());
    }
    if !key.exists() {
        bail!("Tomb key not found: {}", key.display());
    }

    let store = store_dir();
    let was_mounted = store.exists();
    if was_mounted {
        // Quiet close — suppress fallback noise from hdiutil detach
        _ = cmd_close(None, true, verbose);
    }

    let pass_bytes = gpg_decrypt(&key)?;
    let pass = String::from_utf8(pass_bytes).context("Tomb key is not valid UTF-8")?;
    let pass = pass.trim().to_string();

    _message!("Resizing {} to {}.", tomb.display(), new_size);
    tomb::resize_tomb(&tomb, new_size, pass.as_bytes())?;

    _success!("Tomb resized to {}.", new_size);
    if was_mounted {
        _message!("Re-open with 'pass open'.");
    }

    Ok(())
}

pub fn cmd_chkey(
    new_recipients: &[String],
    quiet: bool,
    verbose: bool,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }
    if verbose {
        unsafe {
            std::env::set_var("PASSWORD_STORE_VERBOSE", "1");
        }
    }

    let tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);

    if !tomb.exists() {
        bail!("Tomb not found: {}", tomb.display());
    }
    if !key.exists() {
        bail!("Tomb key not found: {}", key.display());
    }

    _verbose_tomb!("Key: {}", key.display());
    _verbose_tomb!("Tomb: {}", tomb.display());
    _verbose!("New GPG recipients: {}", new_recipients.join(", "));

    // Decrypt the existing key to get the DMG password
    let pass_bytes = gpg_decrypt(&key)?;
    let pass = String::from_utf8(pass_bytes).context("Tomb key is not valid UTF-8")?;

    // Re-encrypt with new recipients
    _message!("Changing GPG keys for {}.", key.display());
    let _ = std::fs::remove_file(&key);
    gpg_encrypt(pass.trim().as_bytes(), new_recipients, &key)?;

    _success!("GPG keys changed for {}.", key.display());
    _message!("New recipients: {}", new_recipients.join(", "));

    Ok(())
}

pub fn cmd_export(
    output: Option<&str>,
    quiet: bool,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }

    let _tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);

    if !key.exists() {
        bail!("Tomb key not found: {}", key.display());
    }

    if let Some(out) = output {
        let out_path = PathBuf::from(out);
        std::fs::copy(&key, &out_path)
            .with_context(|| format!("Failed to copy key to {}", out_path.display()))?;
        _message!("Tomb key exported to {}.", out_path.display());
    } else {
        // Print key info to stdout for piping/backup
        _message!("Tomb key location: {}", key.display());
        let content = std::fs::read_to_string(&key)?;
        println!("{}", content);
    }

    Ok(())
}

// `pass tomb --import file` — import a tomb key from a backup file.
pub fn cmd_import(
    src: &str,
    quiet: bool,
    tomb_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }

    let _tomb = ensure_dmg(&tomb_path.map_or_else(tomb_file, PathBuf::from));
    let key = key_path.map_or_else(tomb_key, PathBuf::from);

    let src_path = PathBuf::from(src);
    if !src_path.exists() {
        bail!("Source file not found: {}", src_path.display());
    }

    // Validate it looks like a GPG-encrypted key file
    let content = std::fs::read_to_string(&src_path).context("Failed to read source file")?;
    if !content.starts_with("-----BEGIN PGP MESSAGE-----") {
        bail!("Source file does not appear to be a GPG-encrypted key");
    }

    std::fs::copy(&src_path, &key)
        .with_context(|| format!("Failed to copy key to {}", key.display()))?;

    _message!("Tomb key imported to {}.", key.display());

    Ok(())
}

pub fn cmd_install(quiet: bool) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }

    let bin = std::env::current_exe().context("Could not determine binary path")?;
    let bin_dir = bin
        .parent()
        .context("Could not determine binary directory")?;

    // Install to a persistent location outside the DMG
    let ext_dir = dirs_home().join(".local/share/pass-extensions");
    std::fs::create_dir_all(&ext_dir)
        .context("Failed to create persistent extensions directory")?;

    for cmd in &["tomb", "open", "close", "timer"] {
        let wrapper = ext_dir.join(format!("{}.bash", cmd));
        let bin_path = bin_dir.join(format!("pass-{}", cmd));
        let content = format!("#!/bin/bash\nexec {} \"$@\"\n", bin_path.to_string_lossy());
        std::fs::write(&wrapper, &content)
            .with_context(|| format!("Failed to write {}", wrapper.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).ok();
        }
        _message!("Created {}", wrapper.display());
    }

    _message!("");
    _message!("Persistent extension wrappers installed.");
    _message!("Add this to your shell config (~/.zshrc or ~/.bashrc):");
    _message!("  export PASSWORD_STORE_EXTENSIONS_DIR=\"$HOME/.local/share/pass-extensions\"");
    _message!("");
    _message!("Make sure $PASSWORD_STORE_ENABLE_EXTENSIONS is set to \"true\"");

    Ok(())
}

pub fn install_internal(force: bool) -> Result<()> {
    let bin = std::env::current_exe().context("Could not determine binary path")?;
    let bin_dir = bin
        .parent()
        .context("Could not determine binary directory")?;

    let store_dir = store_dir();
    if !store_dir.exists() {
        return Ok(());
    }
    let ext_dir = store_dir.join(".extensions");
    if ext_dir.exists() && !force {
        return Ok(()); // already installed
    }
    std::fs::create_dir_all(&ext_dir).ok();

    for cmd in &["tomb", "open", "close", "timer"] {
        let wrapper = ext_dir.join(format!("{}.bash", cmd));
        if wrapper.exists() && !force {
            continue;
        }
        let bin_path = bin_dir.join(format!("pass-{}", cmd));
        let content = format!("#!/bin/bash\nexec {} \"$@\"\n", bin_path.to_string_lossy());
        std::fs::write(&wrapper, &content).ok();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).ok();
        }
    }
    Ok(())
}
