use crate::dl_upd::Config;
use csv::ReaderBuilder;
use lazy_static::lazy_static;
use log::{error, info, warn};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::{thread, time};
use tokio;
use url::Url;
use walkdir::WalkDir;

const SRC_EXISTS: &str = "Source folder exists, continuing";
const SRC_NEXISTS: &str = "Source folder doesn't exist, aboring";
const FILES_EXIST: &str = "All files exist, continuing";
const FILES_NEXIST: &str = "At least one file doesn't exist, aborting";
const GIT_NUSERNAME: &str = "Git username is not provided, login may fail";
const GIT_NPASSWORD: &str = "Git password is not provided, login may fail";
const SSH_NASKPASS: &str = "SSH askpass is not provided, login may fail";
const WALKDIR_ERR: &str = "Could not walk directory";
const ENV_GIT_USERNAME: &str = "GIT_USERNAME";
const ENV_GIT_PASSWORD: &str = "GIT_PASSWORD";
const ENV_SSH_ASKPASS: &str = "SSH_ASKPASS";
const ENV_GIT_ASKPASS: &str = "GIT_ASKPASS";

lazy_static! {
    static ref GIT_OUT: HashSet<&'static str> = HashSet::from_iter([
        "Failed",
        "failed",
        "Enter passphrase",
        "enter passphrase",
        "Couldn't",
        "Could not",
        "couldn't",
        "could not",
        "Error",
        "error",
        "Traceback",
        "404",
        "Enter passphrase for key",
        "fatal",
        "denied",
    ]);
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum GitMode {
    FETCH,
    CLONE,
}

pub async fn git_config_and_run(conf: Config, mode: GitMode) {
    let src_folder = conf.src_folder.unwrap_or(PathBuf::new());
    let files_to_read = conf.files_to_read.unwrap_or(Vec::<PathBuf>::new());
    let git_username = conf.git_username.unwrap_or("git".to_string());
    let git_password = conf.git_password.unwrap_or("pass".to_string());
    let ssh_askpass = conf.ssh_askpass.unwrap_or("pass".to_string());
    let async_exec = conf.async_exec.unwrap_or(false);

    if src_folder.exists() {
        info!("{}", SRC_EXISTS);
    } else {
        error!("{}: {:#?}", SRC_NEXISTS, src_folder);
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

    let git_username = Arc::new(git_username);
    let git_password = Arc::new(git_password);
    let ssh_askpass = Arc::new(ssh_askpass);

    if !async_exec {
        info!("Updates will run in synchronous mode");
    } else {
        info!("Updates will run in asynchronous mode");
    }

    match mode {
        GitMode::CLONE => {
            let mut files_ne = Vec::<PathBuf>::new();
            for f in files_to_read.iter() {
                if !f.is_file() {
                    files_ne.push(f.clone());
                    error!("{}: {:#?}", FILES_NEXIST, &f);
                }
            }

            if files_ne.len() > 0 {
                return;
            } else {
                info!("{}", FILES_EXIST);
            }

            let repos = read_repo_lists(&src_folder, files_to_read);

            clone_repos(git_username, git_password, ssh_askpass, async_exec, repos).await;
        }
        GitMode::FETCH => {
            walk_fetch(
                src_folder,
                git_username,
                git_password,
                ssh_askpass,
                async_exec,
            )
            .await;
        }
    }
}

async fn walk_fetch(
    src_folder: PathBuf,
    gu: Arc<String>,
    gp: Arc<String>,
    sa: Arc<String>,
    ae: bool,
) {
    for f in WalkDir::new(src_folder).into_iter() {
        match f {
            Ok(fl) => {
                if fl.path().is_dir() {
                    if fl.file_name() == ".git" {
                        let gu = gu.clone();
                        let gp = gp.clone();
                        let sa = sa.clone();

                        if !ae {
                            git_fetch(fl.into_path(), gu, gp, sa).await;
                        } else {
                            tokio::task::spawn(async move {
                                git_fetch(fl.into_path(), gu, gp, sa).await;
                            });
                        }
                    };
                }
            }
            Err(e) => error!("{}: {}", WALKDIR_ERR, e),
        }
    }
}

async fn clone_repos(
    gu: Arc<String>,
    gp: Arc<String>,
    sa: Arc<String>,
    ae: bool,
    rp: Vec<(Url, PathBuf)>,
) {
    for repo in rp {
        let gu = gu.clone();
        let gp = gp.clone();
        let sa = sa.clone();

        if !ae {
            git_clone(repo, gu, gp, sa).await;
        } else {
            tokio::task::spawn(async move {
                git_clone(repo, gu, gp, sa).await;
            });
        }
    }
}

async fn git_clone(rp: (Url, PathBuf), gu: Arc<String>, gp: Arc<String>, sa: Arc<String>) {
    info!("Cloning: {:#?} {:#?}", &rp.0, &rp.1);
    let cmd: std::process::Child = Command::new("git")
        .env(ENV_GIT_USERNAME, gu.as_str())
        .env(ENV_GIT_PASSWORD, gp.as_str())
        .env(ENV_SSH_ASKPASS, sa.as_str())
        .env(ENV_GIT_ASKPASS, sa.as_str())
        .arg("clone")
        .arg("--recursive")
        .stdout(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute child");

    control_process(cmd, rp.0.as_str(), GitMode::CLONE);
}

async fn git_fetch(cd: PathBuf, gu: Arc<String>, gp: Arc<String>, sa: Arc<String>) {
    // Move out the .git folder
    cd.clone().pop();

    info!("Updating: {}", cd.to_string_lossy());
    let cmd: std::process::Child = Command::new("git")
        .current_dir(&cd)
        .env(ENV_GIT_USERNAME, gu.as_str())
        .env(ENV_GIT_PASSWORD, gp.as_str())
        .env(ENV_SSH_ASKPASS, sa.as_str())
        .env(ENV_GIT_ASKPASS, sa.as_str())
        .arg("fetch")
        .arg("--all")
        .arg("--tags")
        .arg("--auto-gc")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to execute child");

    control_process(cmd, cd.to_str().unwrap_or(""), GitMode::FETCH);
}

fn control_process(cmd: std::process::Child, repo: &str, mode: GitMode) {
    let pid = cmd.id();
    let time_sleep = time::Duration::from_secs(10);
    thread::sleep(time_sleep);

    let read_stdout = BufReader::new(cmd.stdout.expect("Failed to get stdout"));
    let read_stderr = BufReader::new(cmd.stderr.expect("Failed to get stderr"));

    let mut out: Vec<String> = read_stdout
        .lines()
        .map(|l| l.unwrap_or("".to_string()))
        .collect();
    let err: Vec<String> = read_stderr
        .lines()
        .map(|l| l.unwrap_or("".to_string()))
        .collect();

    out.extend(err);

    for l in out {
        info!("Git output: {}: {}", &repo, l.replace('\n', "  "));
        if HashSet::from_iter(l.split([' ', ':']))
            .intersection(&GIT_OUT)
            .count()
            > 0
        {
            match &mode {
                GitMode::FETCH => {
                    warn!("Problem fetching: {}", &repo);
                }
                GitMode::CLONE => {
                    warn!("Problem cloning: {}", &repo);
                }
            }
            // Kill the process
            let _kill = Command::new("kill").arg("-9").arg(pid.to_string()).output();
            return;
        }
    }

    match &mode {
        GitMode::FETCH => {}
        GitMode::CLONE => {}
    }
}

fn read_repo_lists(sd: &PathBuf, fl: Vec<PathBuf>) -> Vec<(Url, PathBuf)> {
    let mut url_vs_folder = Vec::<(Url, PathBuf)>::with_capacity(2048);

    for f in fl {
        if f.exists() {
            let ext = f.extension().unwrap_or(OsStr::new(""));
            if ext == "txt" || ext == "" {
                if let Some(li) = read_list_txt(sd, f) {
                    url_vs_folder.extend(li);
                }
            } else if ext == "csv" {
                if let Some(li) = read_list_csv(sd, f) {
                    url_vs_folder.extend(li);
                }
            }
        }
    }

    url_vs_folder
}

fn read_list_txt(sd: &PathBuf, txt: PathBuf) -> Option<Vec<(Url, PathBuf)>> {
    let repo_path = sd.clone();
    let file = match File::open(&txt) {
        Ok(fi) => fi,
        Err(e) => {
            error!("Could not read: {:#?} {}", &txt, e);
            return None;
        }
    };

    let reader = BufReader::new(file);
    let lines = reader.lines();
    let mut url_vs_folder = Vec::<(Url, PathBuf)>::with_capacity(512);

    for line in lines {
        let l = match line {
            Ok(li) => li,
            Err(e) => {
                warn!("Could not parse line in txt file: {}", e);
                continue;
            }
        };

        let url: Url = match Url::parse(&l) {
            Ok(u) => u,
            Err(e) => {
                warn!("Could not parse url: {}", e);
                continue;
            }
        };

        let mut cwd = repo_path.clone();
        let url_segments: Vec<&str> = url.path().split('/').collect();
        // https://some.site.com/author/repository - basically 2 segments are
        // present, but there can be other cases, then path will be longer
        cwd.extend(url_segments.into_iter());

        url_vs_folder.push((url, cwd));
    }

    if url_vs_folder.len() > 0 {
        Some(url_vs_folder)
    } else {
        None
    }
}

fn read_list_csv(sd: &PathBuf, csv: PathBuf) -> Option<Vec<(Url, PathBuf)>> {
    let repo_path = sd.clone();
    let file = match File::open(&csv) {
        Ok(fi) => fi,
        Err(e) => {
            error!("Could not read: {:#?} {}", &csv, e);
            return None;
        }
    };

    let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);

    let headers = match reader.headers() {
        Ok(hs) => hs,
        Err(e) => {
            warn!("Could not get header from csv: {}", e);
            return None;
        }
    };

    // There is possibility url entries won't be found, but csv file will be
    // read anyway, maybe it's better to check "repository" or any other
    // record is in header
    let repo_pos = headers
        .iter()
        .position(|he| he.to_string() == "repository".to_string())
        .unwrap_or_default();

    let mut url_vs_folder = Vec::<(Url, PathBuf)>::with_capacity(512);
    for rec in reader.into_records() {
        let r = match rec {
            Ok(re) => re,
            Err(e) => {
                warn!("Could not get record: {}", e);
                continue;
            }
        };
        if r.is_empty() {
            continue;
        }
        let repo = match r.get(repo_pos) {
            Some(re) => re,
            None => {
                warn!("Could not get record element");
                continue;
            }
        };

        let url: Url = match Url::parse(repo) {
            Ok(ur) => ur,
            Err(e) => {
                warn!("Could not parse url: {}", e);
                continue;
            }
        };

        let mut cwd = repo_path.clone();
        let url_segments: Vec<&str> = url.path().split('/').collect();
        // See read_list_txt
        cwd.extend(url_segments.into_iter());

        url_vs_folder.push((url, cwd));
    }

    if url_vs_folder.len() > 0 {
        Some(url_vs_folder)
    } else {
        None
    }
}
