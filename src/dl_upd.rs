//! This module is processing Config from .json file to ensure data is valid before passing
//! it to update(fetch) and download(clone) git functions.

use crate::git_ops::{git_config_and_run, GitMode};
use clap::ArgMatches;
use core::fmt;
use lazy_static::lazy_static;
use log::{error, info};
use serde::Deserialize;
use serde_json;
use std::{
    fmt::Display,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::RwLock,
};

lazy_static! {
    pub static ref CONFIG: RwLock<Config> = RwLock::new(Config::default());
}

/// Config path formatting str
const CPATH: &str = "Config path:";
/// Source folder path formatting str
const SFOLD: &str = "Source folder:";
/// Files to read formatting str
const FLTRD: &str = "Files to read:";
/// Git username formatting str
const GUSER: &str = "Git username:";
/// Git password formatting str
const GPASS: &str = "Git password:";
/// SSH askpass formatting str
const SPASS: &str = "SSH askpass:";
/// Async exec formatting str
const AEXEC: &str = "Async execution:";

/// Passes actual config data to update/fetch function.
pub fn update_directories(matches: ArgMatches) {
    update_config(&matches);
    let conf = get_config();
    info!("Configuration: {}", conf);
    git_config_and_run(conf, GitMode::FETCH);
}

/// Passes actual config data to download/clone function.
pub fn download_repos(matches: ArgMatches) {
    update_config(&matches);
    let conf = get_config();
    info!("Configuration: {}", conf);
    git_config_and_run(conf, GitMode::CLONE);
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub config_path: Option<PathBuf>,
    pub src_folder: Option<PathBuf>,
    pub files_to_read: Option<Vec<PathBuf>>,
    pub git_username: Option<String>,
    pub git_password: Option<String>,
    pub ssh_askpass: Option<String>,
    pub async_exec: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_path: Some(PathBuf::with_capacity(256)),
            src_folder: Some(PathBuf::with_capacity(256)),
            files_to_read: Some(Vec::<PathBuf>::with_capacity(16)),
            git_username: Some(String::with_capacity(16)),
            git_password: Some(String::with_capacity(16)),
            ssh_askpass: Some(String::with_capacity(16)),
            async_exec: Some(false),
        }
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {:#?} {}: {:#?} {}: {:#?} {}: {} {}: {} {}: {} {}: {}",
            CPATH,
            self.config_path.clone().unwrap(),
            SFOLD,
            self.src_folder.clone().unwrap(),
            FLTRD,
            self.files_to_read.clone().unwrap(),
            GUSER,
            self.git_username.clone().unwrap(),
            GPASS,
            self.git_password.clone().unwrap(),
            SPASS,
            self.ssh_askpass.clone().unwrap(),
            AEXEC,
            self.async_exec.clone().unwrap()
        )
    }
}

/// Reads config data and clones it to pass to another functions.
pub fn get_config() -> Config {
    let conf = &CONFIG.read().unwrap();
    Config {
        config_path: conf.config_path.clone(),
        src_folder: conf.src_folder.clone(),
        files_to_read: conf.files_to_read.clone(),
        git_username: conf.git_username.clone(),
        git_password: conf.git_password.clone(),
        ssh_askpass: conf.ssh_askpass.clone(),
        async_exec: conf.async_exec.clone(),
    }
}

fn update_config(matches: &ArgMatches) {
    // debug!("Unlocking config");
    let upd = &mut CONFIG.write().unwrap();
    upd.config_path = Some(PathBuf::from(matches.value_of("config").unwrap()));
    let uconf = read_config(matches);
    upd.src_folder = uconf.src_folder;
    upd.files_to_read = uconf.files_to_read;
    upd.git_username = uconf.git_username;
    upd.git_password = uconf.git_password;
    upd.ssh_askpass = uconf.ssh_askpass;
    upd.async_exec = uconf.async_exec;
}

fn read_config(matches: &ArgMatches) -> Config {
    let filep = Path::new(matches.value_of("config").unwrap());
    let content = read_json(filep);

    return Config {
        config_path: content.config_path,
        src_folder: content.src_folder,
        files_to_read: content.files_to_read,
        git_username: content.git_username,
        git_password: content.git_password,
        ssh_askpass: content.ssh_askpass,
        async_exec: content.async_exec,
    };
}

fn read_json<P: AsRef<Path>>(path: P) -> Config {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            error!("Could not read file: {}", e);
            panic!("Could not read config file!");
        }
    };
    let reader = BufReader::new(file);
    let conf = serde_json::from_reader(reader);
    match conf {
        Ok(c) => return c,
        Err(e) => {
            error!("Could not deserialize .json: {}", e);
            panic!("Could not deserialize .json");
        }
    }
}
