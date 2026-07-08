# Usage
## Setup
Assuming you're using this `ssh` command:
```bash
ssh -i identity_file username@ip_address
```
Make sure you have a file `kafka-term-config` in your working directory:

```
# Accepts relative and absolute paths, do not quote 
ssh_identity_file={ssh_identity}

username={ssh_username}

ip_address={server_ip}

# True if it should retry access with sudo if directory access is denied
use_sudo={true_or_false}
```
## Command
Do `cargo run --release`. Note: performance is terrible on non-release builds.
