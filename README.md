# Pass Tomb OSX
A `pass` extension for macOS that keeps your password tree encrypted inside a tomb - an AES - 256 encrypted DMG managed with `hdiutil`

This project is just a clone of [pass-tomb](https://github.com/roddhjav/pass-tomb). It is only porting over the project to be used on macOS.

## Installation 
### Homebrew (recommended)

 ```bash
brew tap Yahddyyp/tap
brew install pass-tomb
 ```

### Nix Flake

Add to your flake inputs:
```nix
pass-tomb-osx = {
  url = "github:Yahddyyp/pass-tomb-osx";
};
```

Then reference the package in `home.packages` or `environment.systemPackages`:
```nix
pass-tomb-osx.packages.${pkgs.stdenv.hostPlatform.system}.default
```

### Cargo

 ```bash
cargo install --git https://github.com/Yahddyyp/pass-tomb-osx
 ```

You need `gnupg` and `pass` for cargo as well 

## Setup 
### Zsh/Bash  
Add this to your `.zshrc` or `.bashrc`
``` bash 
export PASSWORD_STORE_ENABLE_EXTENSIONS=true
export PASSWORD_STORE_EXTENSIONS_DIR="$HOME/.local/share/pass-extensions"
```

### Fish 
Add this to your `config.fish`
```fish 
set -x PASSWORD_STORE_ENABLE_EXTENSIONS true
set -x PASSWORD_STORE_EXTENSIONS_DIR "$HOME/.local/share/pass-extensions"
```

### nushell 
Add this to your `env.nu` or wherever you have your nu config
```nu
$env.PASSWORD_STORE_ENABLE_EXTENSIONS = "true"
$env.PASSWORD_STORE_EXTENSIONS_DIR = "$HOME/.local/share/pass-extensions"
```

### Other shells 
For other shells, set these environment variables however your shell prefers:
``` sh 
PASSWORD_STORE_ENABLE_EXTENSIONS=true
PASSWORD_STORE_EXTENSIONS_DIR="$HOME/.local/share/pass-extensions"
```


## Usage 
### Create a tomb
By default, this creates `~/.password.tomb.dmg` with key `~/.password.key.tomb` and mounts it at `~/.password-store`:
```bash
pass-tomb <GPG-ID>
```

#### Custom tomb name or location
You can specify custom paths for the DMG and key using `-t/--tomb` and `-k/--key` flags:
```bash
pass-tomb -t ~/Work.tomb.dmg -k ~/Work.key.tomb <GPG-ID>
```

#### Subfolder tomb
To create and mount a tomb for a specific subfolder within your store, use `-p/--path` flag:
```bash
pass-tomb -p work <GPG-ID>
```

### Open the tomb
```bash
pass-open
```

### Use pass as normal
```bash
pass insert github/token
pass show github/token
pass generate github/new-pass 20
```

### Close the tomb
```bash
pass-close
```

### Auto-close timer
You can set an auto-close timer to automatically unmount and close your tomb after a specified duration.

One-shot timer (only applies to the current session):
```bash
pass-open -T 30m
```

Persistent timer (automatically used on every `pass-open` until cleared):
```bash
# Set a persistent timer (supports s, m, h, d, w, e.g. 30m, 1h, 2d, 90s)
pass-timer 30m

# View currently configured persistent timer
pass-timer

# Clear/remove the persistent timer
pass-timer --clear
```

Persistent timer for a specific tomb:
```bash
# Set a timer for a specific tomb image
pass-timer -t ~/.work.tomb.dmg 15m

# View the timer for that specific tomb
pass-timer -t ~/.work.tomb.dmg

# Clear the timer for that specific tomb
pass-timer -t ~/.work.tomb.dmg --clear
```

- **Per-tomb storage:** Each tomb maintains its own persistent timer settings under `~/Library/Application Support/pass-tomb/timers/`. For non-default tombs, pass `-t, --tomb <file>`.
- **Automatic cancellation:** When you manually close an open tomb, any active background auto-close timer for that session is safely cancelled.
- **Worker logs:** Background timer worker output is logged to `~/Library/Application Support/pass-tomb/timers/worker.log`.

### Change password

 ```bash
pass-tomb --change
 ```

### Change GPG recipients

 ```bash
pass-tomb --chkey <new-gpg-id>
 ```

### Resize the DMG
You will need to do this when the DMG fills up as the default is 30m

 ```bash
pass-tomb --resize 100m  
 ```

### Export/import key (backup)

 ```bash
pass-tomb --export ~/backup.tomb-key
pass-tomb --import ~/backup.tomb-key
   ```

## Migration from your password store

1. Backup
```bash 
mv ~/.password-store ~/.password-store.backup
```

2. Create tomb (-n skips init, preserves .gpg-id)
```bash 
pass-tomb -n <GPG-ID>
```

3. Copy everything across
```bash 
cp -a ~/.password-store.backup/. ~/.password-store/
```

4. Verify
```bash
pass show
```

5. Remove backup when satisfied
```bash 
rm -rf ~/.password-store.backup
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PASSWORD_STORE_DIR` | `~/.password-store` | Mount point for the tomb |
| `PASSWORD_STORE_TOMB_FILE` | `~/.password.tomb.dmg` | Path to the encrypted DMG |
| `PASSWORD_STORE_TOMB_KEY` | `~/.password.key.tomb` | Path to the GPG-encrypted tomb key |
| `PASSWORD_STORE_TOMB_SIZE` | `30` | Default tomb size in MB |
| `PASSWORD_STORE_TOMB_TIMER` | `~/Library/Application Support/pass-tomb/timers/<hash>.timer` | Persistent timer file location (legacy `~/.password.tomb.timer` supported as fallback) |
| `PASSWORD_STORE_ENABLE_EXTENSIONS` | — | Must be `true` for `pass-open/close/timer/tomb` |
| `PASSWORD_STORE_EXTENSIONS_DIR` | `$PREFIX/.extensions` | Directory for extension wrapper scripts |
| `PASSWORD_STORE_VERBOSE` | — | Set to `1` for verbose output |
| `PASSWORD_STORE_QUIET` | — | Set to `1` for quiet output |

## Command Flags

### `pass-tomb`

| Flag | Description |
|---|---|
| `-n, --no-init` | Skip password store initialization (skips `pass init`, `.gpg.id` and `git`) |
| `-s, --size <MB>` | Tomb size in MB (default: 30) |
| `-p, --path <subfolder>` | Create tomb for a subfolder |
| `-t, --tomb <file>` | Path to the DMG file |
| `-k, --key <file>` | Path to the key file |
| `-T, --timer <time>` | One-shot timer (e.g. `30m`, `2h`, `2m30s` is not accepted) |
| `-f, --force` | Force overwrite |
| `--unsafe` | Speed up creation (testing only) |
| `-C, --change` | Change the tomb password |
| `--chkey <GPG-ID...>` | Re-encrypt tomb key to new GPG recipients |
| `--resize <SIZE>` | Resize the DMG (e.g. `100m`, `1g`) |
| `--export [file]` | Export tomb key for backup |
| `--import <file>` | Import a tomb key |
| `--install` | Install persistent extension wrappers |

### `pass-open`

| Flag | Description |
|---|---|
| `-t, --tomb <file>` | Path to the DMG |
| `-k, --key <file>` | Path to the key file |
| `-T, --timer <time>` | One-shot auto-close timer (does not persist) |
| `-f, --force` | Force open |

### `pass-timer`

| Flag | Description |
|---|---|
| `<value>` | Set persistent timer (e.g. `30m`, `2h`, `10s`, 2m30s is not accepted) |
| `[store]` | Path to password store |
| `-t, --tomb <file>` | Tomb image whose persistent timer should be read or changed |
| `--clear` | Remove the timer file |
| `-q, --quiet` | Be quiet |
| _(no arg)_ | Show current timer |


## Some other information 
- Make sure to leave the password dir in the tomb (like if you are trying to close it in the same terminal in which you are cd'd into the password dir) or else tomb will fail to close with `pass-close` 

- It will create a random password that it will not show nor can you find anywhere unless you decrypt the key file for the DMG. You can change it to a password you can remember using `-C/--change`, or just leave the key file where the DMG can find it 

- It will use your **gpg key** passphrase to open the tomb and will not need it again for the time you set in your gpg config

- I would make a better `-h/--help` and a `man` page for it but i am too lazy if you want to make this happen you can submit a pr

- This extension also sometimes requires `sudo` password so just know that

- The **raycast** and browser extensions will only work when the tomb is open 

- You might wanna put the `.fseventsd` folder that macOS creates for tracking changes in volumes into `.gitignore`

<p align="center"><a href="https://github.com/yahddyyp/pass-tomb-osx/blob/main/LICENSE"><img src="https://img.shields.io/static/v1.svg?style=for-the-badge&label=License&message=MIT&logoColor=cdd6f4&colorA=1e1e2e&colorB=cba6f7"/></a></p>

