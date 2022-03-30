use crate::dl_upd::get_config;
use log::{error, info, warn};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio;
use walkdir::WalkDir;

const SRC_EXISTS: &str = "Source folder exists, continuing";
const SRC_NEXISTS: &str = "Source folder doesn't exist, aboring";
const GIT_NUSERNAME: &str = "Git username is not provided, login may fail";
const GIT_NPASSWORD: &str = "Git password is not provided, login may fail";
const SSH_NASKPASS: &str = "SSH askpass is not provided, login may fail";
const WALKDIR_ERR: &str = "Could not walk directory";
const ENV_GIT_USERNAME: &str = "GIT_USERNAME";
const ENV_GIT_PASSWORD: &str = "GIT_PASSWORD";
const ENV_SSH_ASKPASS: &str = "SSH_ASKPASS";

pub async fn git_fetch_repos(
    src_folder: PathBuf,
    git_username: Arc<String>,
    git_password: Arc<String>,
    ssh_askpass: Arc<String>,
) {
    if src_folder.exists() {
        info!("{}", SRC_EXISTS);
    } else {
        error!("{}", SRC_NEXISTS);
        return;
    }

    if git_username.len() == 0 {
        info!("{}", GIT_NUSERNAME)
    }
    if git_password.len() == 0 {
        info!("{}", GIT_NPASSWORD)
    }
    if ssh_askpass.len() == 0 {
        info!("{}", SSH_NASKPASS)
    }

    for f in WalkDir::new(src_folder).into_iter() {
        match f {
            Ok(fl) => {
                if fl.path().is_dir() {
                    if fl.file_name() == ".git" {
                        let gu = git_username.clone();
                        let gp = git_password.clone();
                        let sa = ssh_askpass.clone();

                        tokio::spawn(async move {
                            git_fetch(fl.into_path(), gu, gp, sa).await;
                        });
                    };
                }
            }
            Err(e) => error!("{}: {}", WALKDIR_ERR, e),
        }
    }
}

pub async fn git_clone_repos(
    files_to_read: Vec<PathBuf>,
    src_folder: PathBuf,
    git_username: Arc<String>,
    git_password: Arc<String>,
    ssh_askpass: Arc<String>,
) {
}

async fn git_fetch(f: PathBuf, gu: Arc<String>, gp: Arc<String>, sa: Arc<String>) {
    f.clone().pop();
    let conf = get_config().read().unwrap();
    let output = Command::new("git")
        .current_dir(f)
        .env(ENV_GIT_USERNAME, gu.as_str())
        .env(ENV_GIT_PASSWORD, gp.as_str())
        .env(ENV_SSH_ASKPASS, sa.as_str())
        .arg("fetch")
        .arg("--all")
        .arg("--tags")
        .arg("--recurse_submodules");
}
