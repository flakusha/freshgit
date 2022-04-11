# freshgit - git repositories downloader and updater

This is application in Rust to keep your repositories locally and update them
using configuration file.

My aim is to test out tokio and futures for this purpose and achieve better
performance compared to Python prototype which has problems parsing csvs with
malformed utf-8 and huge CPU overload with many tasks running in the Pool.

## Usage

1. Create or edit configuration file:

```json
{
  "files_to_read": ["", "", ""],
  "src_folder": "",
  "git_username": "git",
  "git_password": "your_password",
  "ssh_askpass": "your_password",
  "async_exec": true,
}
```

2. Build from source and run:

`cargo run --release -- -c ./config.json -d` - to download (clone) repositories
`cargo run --release -- -c ./config.json -u` - to update (fetch) repositories

## Supported OS

- Obviously you have to install git :)
- GNU/Linux
- MacOS (not tested)

