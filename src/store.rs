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
fn tomb_size_mb() -> Result<usize> {
    if let Ok(s) = std::env::var("PASSWORD_STORE_TOMB_SIZE") {
        s.parse().with_context(|| {
            format!("Invalid PASSWORD_STORE_TOMB_SIZE: '{s}' (must be a positive integer)")
        })
    } else {
        Ok(30)
    }
}

fn dirs_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// Persistent timer settings are keyed by the canonical tomb image path, so
// different tombs never overwrite one another's defaults. The environment
// override is retained for compatibility with existing installations.
fn stable_tomb_path(tomb_path: &Path) -> PathBuf {
    if let Ok(path) = tomb_path.canonicalize() {
        return path;
    }

    let absolute = if tomb_path.is_absolute() {
        tomb_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(tomb_path))
            .unwrap_or_else(|_| tomb_path.to_path_buf())
    };

    // Canonicalize the nearest existing ancestor too, so paths under symlinked
    // directories (for example /tmp on macOS) retain the same identity before
    // and after the disk image is created.
    let mut ancestor = absolute.clone();
    while !ancestor.exists() {
        if !ancestor.pop() {
            return absolute;
        }
    }
    let suffix = absolute
        .strip_prefix(&ancestor)
        .unwrap_or(Path::new(""))
        .to_path_buf();
    ancestor
        .canonicalize()
        .unwrap_or_else(|_| ancestor.clone())
        .join(suffix)
}

fn tomb_identity_hash(tomb_path: &Path) -> String {
    let canonical = stable_tomb_path(tomb_path);
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in canonical.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn tomb_timer_path(tomb_path: &Path) -> PathBuf {
    if let Ok(f) = std::env::var("PASSWORD_STORE_TOMB_TIMER") {
        return PathBuf::from(f);
    }

    let canonical = stable_tomb_path(tomb_path);
    let identity = tomb_identity_hash(tomb_path);
    let config = dirs::config_dir()
        .unwrap_or_else(dirs_home)
        .join("pass-tomb")
        .join("timers");
    let per_tomb_path = config.join(format!("{identity}.timer"));

    // Preserve the old default-tomb timer file as a compatibility path.
    // Other tombs never consult this legacy file.
    let default_tomb = stable_tomb_path(&ensure_dmg(&tomb_file()));
    if !per_tomb_path.exists() && canonical == default_tomb {
        let legacy_path = dirs_home().join(".password.tomb.timer");
        if legacy_path.exists() {
            return legacy_path;
        }
    }

    per_tomb_path
}

fn tomb_timer_active_path(tomb_path: &Path) -> PathBuf {
    // Runtime state must remain per tomb even when the legacy global timer-file
    // environment override is configured.
    dirs::config_dir()
        .unwrap_or_else(dirs_home)
        .join("pass-tomb")
        .join("timers")
        .join(format!("{}.active", tomb_identity_hash(tomb_path)))
}

fn lock_timer_state(tomb_path: &Path) -> Result<std::fs::File> {
    use std::os::unix::io::AsRawFd;

    let active_path = tomb_timer_active_path(tomb_path);
    let lock_path = active_path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).context("could not create timer state directory")?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("could not open timer lock {}", lock_path.display()))?;
    // `flock` is available on macOS and coordinates close/open/timer workers.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error()).context("could not lock timer state");
    }
    Ok(file)
}

fn cancel_auto_close_timer(tomb_path: &Path) -> Result<()> {
    let _lock = lock_timer_state(tomb_path)?;
    let active_path = tomb_timer_active_path(tomb_path);
    if let Err(e) = std::fs::remove_file(&active_path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(e).with_context(|| {
            format!(
                "could not invalidate auto-close timer {}",
                active_path.display()
            )
        });
    }
    Ok(())
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

    let number = number.trim();
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    number.parse::<u64>().ok()?.checked_mul(multiplier)
}

fn spawn_auto_close_timer(seconds: u64, tomb_path: &Path, mount_point: &Path) -> Result<()> {
    use std::process::Stdio;

    let canonical = stable_tomb_path(tomb_path);
    let mounted = tomb::list_open_tombs()?
        .into_iter()
        .find(|entry| {
            entry.image_path.as_ref().is_some_and(|path| {
                path.canonicalize().unwrap_or_else(|_| path.clone()) == canonical
                    && entry.mount_point == mount_point
            })
        })
        .context("could not verify which disk image is mounted at the requested path")?;
    let device = mounted
        .device_node
        .context("hdiutil did not report the mounted device identifier")?;

    // Serialize timer creation against close/open workers.
    let _lock = lock_timer_state(tomb_path)?;
    // Use the sibling pass-tomb executable as a worker. It verifies the image,
    // mount point, device node, and token again at expiry before detaching.
    let current_exe = std::env::current_exe().context("could not locate current executable")?;
    let mut worker_exe = current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("pass-tomb");
    if !worker_exe.is_file()
        && let Ok(path_var) = std::env::var("PATH")
    {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("pass-tomb");
            if candidate.is_file() {
                worker_exe = candidate;
                break;
            }
        }
    }
    if !worker_exe.is_file() {
        bail!(
            "auto-close worker executable not found at {}",
            worker_exe.display()
        );
    }

    // Every timer gets a token. Replacing/removing this file invalidates older workers.
    let active_path = tomb_timer_active_path(tomb_path);
    if let Some(parent) = active_path.parent() {
        std::fs::create_dir_all(parent).context("could not create timer state directory")?;
    }
    let token = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    std::fs::write(&active_path, &token).context("could not record auto-close timer state")?;

    let log_file = active_path
        .parent()
        .map(|p| p.join("worker.log"))
        .and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
        });

    let stderr_cfg = match log_file {
        Some(file) => Stdio::from(file),
        None => Stdio::null(),
    };

    let child = std::process::Command::new("/usr/bin/nohup")
        .arg(worker_exe)
        .arg("--__auto-close-worker")
        .arg(seconds.to_string())
        .arg(&canonical)
        .arg(mount_point)
        .arg(&device)
        .arg(&active_path)
        .arg(&token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr_cfg)
        .spawn();

    match child {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(active_path);
            Err(error).context("could not start auto-close timer")
        }
    }
}

/// Internal detached timer worker. This is invoked only by the hidden CLI path.
pub fn run_auto_close_worker(
    seconds: u64,
    tomb_path: &Path,
    mount_point: &Path,
    device_node: &Path,
    active_path: &Path,
    token: &str,
) -> Result<()> {
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    let _lock = lock_timer_state(tomb_path)?;

    let Ok(current_token) = std::fs::read_to_string(active_path) else {
        return Ok(());
    };
    if current_token != token {
        return Ok(());
    }

    let canonical = stable_tomb_path(tomb_path);
    let still_same_mount = tomb::list_open_tombs()?.into_iter().any(|entry| {
        entry.image_path.as_ref().is_some_and(|image| {
            image.canonicalize().unwrap_or_else(|_| image.clone()) == canonical
                && entry.mount_point == mount_point
                && entry.device_node.as_deref() == Some(device_node)
        })
    });
    if !still_same_mount {
        let _ = std::fs::remove_file(active_path);
        return Ok(());
    }

    tomb::close_tomb_by_mount(device_node)?;
    let remains_mounted = tomb::list_open_tombs()?.into_iter().any(|entry| {
        entry.image_path.as_ref().is_some_and(|image| {
            image.canonicalize().unwrap_or_else(|_| image.clone()) == canonical
                && entry.mount_point == mount_point
                && entry.device_node.as_deref() == Some(device_node)
        })
    });
    if remains_mounted {
        bail!("auto-close detach returned success, but the original tomb remains mounted");
    }

    if std::fs::read_to_string(active_path).is_ok_and(|value| value == token) {
        let _ = std::fs::remove_file(active_path);
    }
    Ok(())
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
    let trusted = ["m", "f", "u"];

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
    let sz_mb = if let Some(s) = size {
        s.parse::<usize>().with_context(|| {
            format!("Invalid size argument '{s}': must be a positive integer in MB")
        })?
    } else {
        tomb_size_mb()?
    };
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
    validate_gpg_ids(gpg_ids).context("GPG ID validation failed")?;
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
    // Invalidate stale state left by a previous image at this path.
    cancel_auto_close_timer(&tomb)?;

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
    let mut child = match Command::new("hdiutil")
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
    {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&key);
            return Err(e).context("Failed to spawn hdiutil create");
        }
    };

    // Write the tomb password to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(tomb_pass.as_bytes())?;
        stdin.write_all(&[0u8])?;
    }
    let exit = child.wait()?;
    if !exit.success() {
        let _ = std::fs::remove_file(&key);
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

    // Set ownership. If this post-mount step fails, avoid leaving the image mounted.
    if let Err(error) = set_ownership(&mount_point) {
        let _ = tomb::close_tomb_by_mount(&mount_point);
        return Err(error).context("failed to set ownership; attempted to unmount the new tomb");
    }

    // Initialize the password store using pass init (if available) or manually
    if !no_init {
        // Write .gpg-id (equivalent to `pass init gpg-id...`)
        let gpg_id_content = gpg_ids.join("\n");
        if let Err(error) = std::fs::write(mount_point.join(".gpg-id"), &gpg_id_content) {
            let _ = tomb::close_tomb_by_mount(&mount_point);
            return Err(error)
                .context("Failed to write .gpg-id; attempted to unmount the new tomb");
        }

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
        let timer_path = tomb_timer_path(&tomb);

        if timer_path.exists() {
            let value = match std::fs::read_to_string(&timer_path) {
                Ok(value) => value,
                Err(error) => {
                    let _ = tomb::close_tomb_by_mount(&mount_point);
                    return Err(error)
                        .context("failed to read saved timer; attempted to unmount tomb");
                }
            };
            let value = value.trim().to_string();

            if value.is_empty() { None } else { Some(value) }
        } else {
            None
        }
    };

    if let Some(t) = timer_value {
        match parse_timer_seconds(&t) {
            Some(secs) if secs > 0 => {
                if let Err(error) = spawn_auto_close_timer(secs, &tomb, &mount_point) {
                    eprintln!("Warning: auto-close timer was not started: {error:#}");
                } else {
                    timed = Some(t);
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
    if let Some(mp) = tomb::find_mount_point(&tomb)? {
        _message!(
            "The password tomb {} is already open at {}.",
            tomb.display(),
            mp.display()
        );
        return Ok(());
    }
    // Prevent a timer left by an externally closed session from affecting this one.
    cancel_auto_close_timer(&tomb)?;

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

    // Set ownership. If this post-mount step fails, avoid leaving the image mounted.
    if let Err(error) = set_ownership(&mount_point) {
        let _ = tomb::close_tomb_by_mount(&mount_point);
        return Err(error).context("failed to set ownership; attempted to unmount the opened tomb");
    }

    let mut timed = None;

    let timer_value = if let Some(t) = timer {
        Some(t.to_string())
    } else {
        let timer_path = tomb_timer_path(&tomb);

        if timer_path.exists() {
            let value = match std::fs::read_to_string(&timer_path) {
                Ok(value) => value,
                Err(error) => {
                    let _ = tomb::close_tomb_by_mount(&mount_point);
                    return Err(error)
                        .context("failed to read saved timer; attempted to unmount tomb");
                }
            };
            let value = value.trim().to_string();

            if value.is_empty() { None } else { Some(value) }
        } else {
            None
        }
    };

    if let Some(t) = timer_value {
        match parse_timer_seconds(&t) {
            Some(secs) if secs > 0 => {
                if let Err(error) = spawn_auto_close_timer(secs, &tomb, &mount_point) {
                    eprintln!("Warning: auto-close timer was not started: {error:#}");
                } else {
                    timed = Some(t);
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

    let open_tombs = tomb::list_open_tombs()?;
    let entry = if let Some(argument) = store_path {
        let requested = PathBuf::from(argument);
        if requested
            .extension()
            .is_some_and(|extension| extension == "dmg")
        {
            let canonical = stable_tomb_path(&requested);
            open_tombs.into_iter().find(|entry| {
                entry
                    .image_path
                    .as_ref()
                    .is_some_and(|image| stable_tomb_path(image) == canonical)
            })
        } else {
            let canonical_mount = stable_tomb_path(&requested);
            open_tombs.into_iter().find(|entry| {
                entry.volume_name == argument
                    || stable_tomb_path(&entry.mount_point) == canonical_mount
            })
        }
    } else {
        let requested = ensure_dmg(&tomb_file());
        let canonical = stable_tomb_path(&requested);
        let canonical_store = stable_tomb_path(&store_dir());
        open_tombs
            .iter()
            .find(|entry| {
                entry
                    .image_path
                    .as_ref()
                    .is_some_and(|image| stable_tomb_path(image) == canonical)
            })
            .or_else(|| {
                // Fallback: match by the active password store directory (~/.password-store)
                open_tombs
                    .iter()
                    .find(|entry| stable_tomb_path(&entry.mount_point) == canonical_store)
            })
            .or_else(|| {
                // Fallback: match default volume name ("pass-tomb")
                open_tombs
                    .iter()
                    .find(|entry| entry.volume_name == tomb::DEFAULT_VOLUME_NAME)
            })
            .cloned()
    };

    let Some(entry) = entry else {
        let target = store_path
            .map(str::to_owned)
            .unwrap_or_else(|| tomb_file().display().to_string());
        bail!(
            "No mounted tomb matched '{}'; no other volume was detached.",
            target
        );
    };
    let image_path = entry
        .image_path
        .clone()
        .context("mounted image path is unavailable; refusing to close an unidentified volume")?;

    // Invalidate the timer before detaching, so its worker cannot act on a later session.
    cancel_auto_close_timer(&image_path)?;
    let canonical = stable_tomb_path(&image_path);
    // The timer worker may have completed while close waited for the lock.
    let Some(entry) = tomb::list_open_tombs()?.into_iter().find(|current| {
        current
            .image_path
            .as_ref()
            .is_some_and(|image| stable_tomb_path(image) == canonical)
    }) else {
        _success!("Your password tomb has been closed.");
        return Ok(());
    };
    let detach_target = entry.device_node.as_deref().unwrap_or(&entry.mount_point);
    tomb::close_tomb_by_mount(detach_target)?;

    let still_mounted = tomb::list_open_tombs()?.iter().any(|current| {
        current
            .image_path
            .as_ref()
            .is_some_and(|image| stable_tomb_path(image) == canonical)
    });
    if still_mounted {
        bail!(
            "hdiutil reported success, but {} is still mounted.",
            image_path.display()
        );
    }

    _success!("Your password tomb has been closed.");
    _message!("Your passwords remain present in {}.", image_path.display());
    Ok(())
}

pub fn cmd_timer(
    timer_value: Option<&str>,
    store_path: Option<&str>,
    tomb_path: Option<&str>,
    clear: bool,
    quiet: bool,
) -> Result<()> {
    if quiet {
        unsafe {
            std::env::set_var("PASSWORD_STORE_QUIET", "1");
        }
    }

    let target_spec = tomb_path.or(store_path);
    let timer_target = ensure_dmg(&target_spec.map_or_else(tomb_file, PathBuf::from));
    let timer_path = tomb_timer_path(&timer_target);

    let is_clear = clear || timer_value.is_some_and(|v| v.eq_ignore_ascii_case("clear"));

    // Clear the persistent timer setting.
    if is_clear {
        if timer_path.exists() {
            std::fs::remove_file(&timer_path).context("Failed to clear persistent timer")?;
            _message!("Persistent timer cleared for {}.", timer_target.display());
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

        _message!(
            "Persistent timer for {} set to {}.",
            timer_target.display(),
            val.trim()
        );
        return Ok(());
    }

    // Display the current persistent timer setting.
    if timer_path.exists() {
        let content =
            std::fs::read_to_string(&timer_path).context("Failed to read persistent timer")?;
        let saved_timer = content.trim();

        if saved_timer.is_empty() {
            _message!("No persistent timer set.");
        } else {
            _message!(
                "Persistent timer for {}: {}",
                timer_target.display(),
                saved_timer
            );
            println!("{}", saved_timer);
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
    // Change DMG image password first so we don't break key synchronization if chpass fails
    tomb::change_tomb_password(&tomb, old_pass.as_bytes(), new_pass.as_bytes())?;

    // Now re-encrypt the key file using a temporary file and atomic rename
    let tmp_key = key.with_extension("tmp");
    if let Err(e) = gpg_encrypt(new_pass.as_bytes(), &recipients, &tmp_key) {
        // Revert DMG password back to old password if key file update fails
        let _ = tomb::change_tomb_password(&tomb, new_pass.as_bytes(), old_pass.as_bytes());
        return Err(e).context("Failed to encrypt new key file; reverted DMG password to previous");
    }
    std::fs::rename(&tmp_key, &key)
        .with_context(|| format!("Failed to update key file at {}", key.display()))?;

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

    let was_mounted = tomb::find_mount_point(&tomb)?.is_some();
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

    // Re-encrypt with new recipients via a temporary file and atomic rename
    _message!("Changing GPG keys for {}.", key.display());
    let tmp_key = key.with_extension("tmp");
    gpg_encrypt(pass.trim().as_bytes(), new_recipients, &tmp_key)?;
    std::fs::rename(&tmp_key, &key)
        .with_context(|| format!("Failed to update key file at {}", key.display()))?;

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

    // Always install to the persistent user extension directory so pass extensions
    // remain accessible even when the tomb is unmounted.
    let user_ext_dir = dirs_home().join(".local/share/pass-extensions");
    std::fs::create_dir_all(&user_ext_dir).ok();
    for cmd in &["tomb", "open", "close", "timer"] {
        let wrapper = user_ext_dir.join(format!("{}.bash", cmd));
        if !wrapper.exists() || force {
            let bin_path = bin_dir.join(format!("pass-{}", cmd));
            let content = format!("#!/bin/bash\nexec {} \"$@\"\n", bin_path.to_string_lossy());
            if std::fs::write(&wrapper, &content).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).ok();
                }
            }
        }
    }

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

#[cfg(test)]
mod timer_tests {
    use super::parse_timer_seconds;

    #[test]
    fn parses_supported_timer_units_case_insensitively() {
        let cases = [
            ("30", 30),
            ("30s", 30),
            ("30sec", 30),
            ("30 seconds", 30),
            ("2m", 120),
            ("2min", 120),
            ("2 minutes", 120),
            ("3h", 10_800),
            ("1d", 86_400),
            ("2w", 1_209_600),
            (" 5 MIN ", 300),
            ("1WEEK", 604_800),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_timer_seconds(input), Some(expected), "input: {input}");
        }
    }

    #[test]
    fn different_tomb_paths_have_independent_timer_identities() {
        use std::path::Path;
        assert_ne!(
            super::tomb_identity_hash(Path::new("/tmp/personal.dmg")),
            super::tomb_identity_hash(Path::new("/tmp/work.dmg")),
        );
    }

    #[test]
    fn rejects_invalid_and_overflowing_timer_values() {
        for input in [
            "",
            " ",
            "0x",
            "-1",
            "+1",
            "1.5m",
            "abc",
            "1fortnight",
            "18446744073709551615w",
        ] {
            assert_eq!(parse_timer_seconds(input), None, "input: {input}");
        }
        assert_eq!(parse_timer_seconds("0"), Some(0));
        assert_eq!(parse_timer_seconds("0s"), Some(0));
    }
}
