//! This is the main module responsible for cloning and fetching repositories.
//! Overall scenario is to accept configuration, check it and then run
//! corresponding process. While process is running, it's stdout and stderr are
//! captured to check if transaction is possible and if it's not there is an
//! attempt to kill process to free the runtime slot for new process.
use crate::dl_upd::Config;
use csv::ReaderBuilder;
use futures::future::join_all;
use lazy_static::lazy_static;
use log::{error, info, warn};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::{self, runtime};
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

/// Checks passed configuration and also sets sane defaults before running clone
/// or getch processes.
pub fn git_config_and_run(conf: Config, mode: GitMode) {
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

            clone_repos(git_username, git_password, ssh_askpass, async_exec, repos);
        }
        GitMode::FETCH => {
            walk_fetch(
                src_folder,
                git_username,
                git_password,
                ssh_askpass,
                async_exec,
            );
        }
    }
}

/// Creates custom tokio runtime for sync and async execution.
fn create_tokio_runtime(ae: bool) -> runtime::Runtime {
    match ae {
        true => runtime::Builder::new_multi_thread()
            .worker_threads(32)
            .max_blocking_threads(32)
            .thread_name("tokio-runtime-git-fetch-multi")
            .on_thread_start(|| {
                info!("Runtime started");
            })
            .on_thread_stop(|| {
                info!("Runtime finished working");
            })
            .enable_all()
            .build()
            .unwrap(),
        false => runtime::Builder::new_current_thread()
            .worker_threads(1)
            .max_blocking_threads(1)
            .thread_name("tokio-runtime-git-fetch-current")
            .on_thread_start(|| info!("Single thread runtime started"))
            .on_thread_stop(|| info!("Single thread runtime finished working"))
            .enable_all()
            .build()
            .unwrap(),
    }
}

/// Walks all the folders in provided root folder and tries to check out changes if git repository
/// is detected.
fn walk_fetch(src_folder: PathBuf, gu: Arc<String>, gp: Arc<String>, sa: Arc<String>, ae: bool) {
    let rt = create_tokio_runtime(ae);
    let mut jhs: Vec<JoinHandle<_>> = vec![];

    for f in WalkDir::new(src_folder).into_iter() {
        match f {
            Ok(fl) => {
                if fl.path().is_dir() {
                    if fl.file_name() == ".git" {
                        let gu = gu.clone();
                        let gp = gp.clone();
                        let sa = sa.clone();

                        if !ae {
                            rt.block_on(async {
                                git_fetch(fl.into_path(), gu, gp, sa).await;
                            })
                        } else {
                            let jh = rt.spawn(async {
                                tokio::task::spawn(git_fetch(fl.into_path(), gu, gp, sa));
                            });
                            jhs.push(jh);
                        }
                    };
                }
            }
            Err(e) => error!("{}: {}", WALKDIR_ERR, e),
        }
    }

    if jhs.len() > 0 {
        rt.block_on(async {
            join_all(jhs).await;
        })
    }
}

/// Clones provided repositories using sync or async tokio runtimes.
fn clone_repos(
    gu: Arc<String>,
    gp: Arc<String>,
    sa: Arc<String>,
    ae: bool,
    rp: Vec<(Url, PathBuf)>,
) {
    let rt = create_tokio_runtime(ae);
    let mut jhs: Vec<JoinHandle<_>> = vec![];

    for repo in rp {
        let gu = gu.clone();
        let gp = gp.clone();
        let sa = sa.clone();

        if !ae {
            rt.block_on(async {
                git_clone(repo, gu, gp, sa).await;
            })
        } else {
            let jh = rt.spawn(async {
                tokio::task::spawn(git_clone(repo, gu, gp, sa));
            });
            jhs.push(jh);
        }
    }

    if jhs.len() > 0 {
        rt.block_on(async {
            join_all(jhs).await;
        })
    }
}

/// Clones provided repository using tokio::process::Command.
async fn git_clone(rp: (Url, PathBuf), gu: Arc<String>, gp: Arc<String>, sa: Arc<String>) {
    if rp.1.exists() && rp.1.is_dir() {
        info!(
            "Repository is already cloned, use update instead: {}",
            rp.1.to_str().unwrap_or("error unwrapping")
        );
        return;
    }

    info!("Cloning: {} {}", &rp.0.to_string(), &rp.1.to_str().unwrap());
    let cmd: tokio::process::Child = Command::new("git")
        .env(ENV_GIT_USERNAME, gu.as_str())
        .env(ENV_GIT_PASSWORD, gp.as_str())
        .env(ENV_SSH_ASKPASS, sa.as_str())
        .env(ENV_GIT_ASKPASS, sa.as_str())
        .arg("clone")
        .arg("--recursive")
        .arg(format!("{}", rp.0))
        .arg(rp.1)
        .stdout(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to execute child");

    let _res = control_process(cmd, rp.0.as_str(), GitMode::CLONE).await;
}

/// Fetches detected repository using tokio::process::Command.
async fn git_fetch(cd: PathBuf, gu: Arc<String>, gp: Arc<String>, sa: Arc<String>) {
    if cd.exists() && cd.is_dir() {
        info!("Updating: {}", cd.to_string_lossy());
    } else {
        return;
    }

    // Move out the .git folder
    cd.clone().pop();

    let cmd: tokio::process::Child = Command::new("git")
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

    // No, I'm not going to use it
    let _res = control_process(cmd, cd.to_str().unwrap_or(""), GitMode::FETCH).await;
}

/// Tries to get stdout and stderr from running process and then checks them for
/// clone or fetch failure to kill process and do not wait for user input
/// this is usually needed when user password is incorrect, not accepted or
/// repository does not exist at all, or cannot answer.
async fn control_process(
    mut cmd: tokio::process::Child,
    repo: &str,
    mode: GitMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let pid = cmd.id().unwrap();
    let stdout = cmd.stdout.take().expect("no stdout");
    let stderr = cmd.stderr.take().expect("no stderr");
    let reader_out = BufReader::new(stdout);
    let reader_err = BufReader::new(stderr);

    tokio::task::spawn(async move {
        let status = cmd.wait().await.expect("Process failed");
        info!("Finished process: {}", status);
    });

    check_process(pid, reader_out, reader_err, &mode, repo).await
}

/// Continuously parses stdout and stderr from the Command to detect if it is
/// running correctly, otherwise sends kill -9 for this pid.
async fn check_process(
    pid: u32,
    reader_out: BufReader<tokio::process::ChildStdout>,
    reader_err: BufReader<tokio::process::ChildStderr>,
    mode: &GitMode,
    repo: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader_o = reader_out.lines();
    let mut reader_e = reader_err.lines();

    while let (Some(o), Some(e)) = (reader_o.next_line().await?, reader_e.next_line().await?) {
        // debug!("Stdout: {}; Stderr: {}", o, e);
        if HashSet::from_iter(o.split([' ', ':']))
            .intersection(&GIT_OUT)
            .count()
            > 0
            || HashSet::from_iter(e.split([' ', ':']))
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
            info!("Killed process: {}", pid);
        }
    }

    Ok(())
}

/// Reads repo lists from provided files. Text files with repository on every line and
/// csv file with "repositories" column are supported at the moment.
fn read_repo_lists(sd: &PathBuf, fl: Vec<PathBuf>) -> Vec<(Url, PathBuf)> {
    let mut url_vs_folder = Vec::<(Url, PathBuf)>::with_capacity(2048);

    for f in fl {
        if f.exists() {
            let ext = f.extension().unwrap_or(OsStr::new(""));
            if let Some(li) = read_lists(sd, &f, &ext) {
                url_vs_folder.extend(li);
            }
        }
    }

    url_vs_folder
}

/// Reads provided text file and converts every supported entry to pair of
/// (Entry_URL, Folder_Addr) pushed into Vec.
fn read_lists(sd: &PathBuf, txt: &PathBuf, ext: &OsStr) -> Option<Vec<(Url, PathBuf)>> {
    let repo_path = sd.clone();

    let file = match File::open(&txt) {
        Ok(fi) => fi,
        Err(e) => {
            error!("Could not read: {} {}", &txt.to_str().unwrap(), e);
            return None;
        }
    };

    let mut url_vs_folder = Vec::<(Url, PathBuf)>::with_capacity(4096);

    if ext == "txt" || ext == "" {
        let reader = std::io::BufReader::new(file);
        let lines: Vec<String> = reader
            .lines()
            .map(|l| l.unwrap_or("".to_string()))
            .collect();

        for l in lines.into_iter() {
            // debug!("String URL (txt): {}", l);
            let url: Url = match Url::parse(&l) {
                Ok(u) => u,
                Err(e) => {
                    warn!("Could not parse url: {} {}", l, e);
                    continue;
                }
            };

            let mut cwd = repo_path.clone();
            // https://some.site.com/author/repository - basically 2 segments are
            // present, but there can be other cases, then path will be longer
            let mut url_segments: Vec<String> =
                url.path().split('/').map(|i| i.to_string()).collect();
            let idx_last = url_segments.len() - 1;
            // Cut ".git" from the end of path
            if url_segments.get(idx_last).unwrap().ends_with(".git") {
                url_segments[idx_last] =
                    url_segments.get_mut(idx_last).unwrap().replace(".git", "");
            }

            cwd.extend(url_segments.into_iter());

            url_vs_folder.push((url, cwd));
        }
    } else if ext == "csv" {
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

            // debug!("String URL (csv): {}", repo);
            let url: Url = match Url::parse(repo) {
                Ok(ur) => ur,
                Err(e) => {
                    warn!("Could not parse url: {} {}", repo, e);
                    continue;
                }
            };

            let mut cwd = repo_path.clone();
            let mut url_segments: Vec<String> =
                url.path().split('/').map(|i| i.to_string()).collect();
            let idx_last = url_segments.len() - 1;
            // Cut ".git" from the end of path
            if url_segments.get(idx_last).unwrap().ends_with(".git") {
                url_segments[idx_last] = url_segments.get(idx_last).unwrap().replace(".git", "");
            }
            // See read_list_txt
            cwd.extend(url_segments.into_iter());

            url_vs_folder.push((url, cwd));
        }
    }

    if url_vs_folder.len() > 0 {
        Some(url_vs_folder)
    } else {
        None
    }
}
