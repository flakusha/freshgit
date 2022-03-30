use crate::git_ops::{git_clone_repos, git_fetch_repos};
use clap::ArgMatches;
use lazy_static::lazy_static;
use log::{error, info, warn};
use serde::Deserialize;
use serde_json;
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

lazy_static! {
    pub static ref CONFIG: RwLock<Config> = RwLock::new(Config::default());
}

pub async fn update_directories() {
    let conf = get_config().read().unwrap();
    info!("Config: {:?}", conf);

    // Clone values one time to avoid cloning them again
    // and again inside the loop with async move
    git_fetch_repos(
        conf.src_folder.clone().unwrap_or_else(|| PathBuf::new()),
        Arc::new(conf.git_username.clone().unwrap_or("".to_string())),
        Arc::new(conf.git_password.clone().unwrap_or("".to_string())),
        Arc::new(conf.ssh_askpass.clone().unwrap_or("".to_string())),
    )
    .await;
}

pub async fn download_repos() {
    let conf = get_config().read().unwrap();
    info!("Config: {:?}", conf);
    git_clone_repos(conf).await;
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub config_path: Option<PathBuf>,
    pub src_folder: Option<PathBuf>,
    pub files_to_read: Option<Vec<PathBuf>>,
    pub git_username: Option<String>,
    pub git_password: Option<String>,
    pub ssh_askpass: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_path: Some(PathBuf::with_capacity(256)),
            src_folder: Some(PathBuf::with_capacity(256)),
            files_to_read: Some(Vec::with_capacity(16)),
            git_username: Some(String::with_capacity(16)),
            git_password: Some(String::with_capacity(16)),
            ssh_askpass: Some(String::with_capacity(16)),
        }
    }
}

pub fn get_config() -> &'static CONFIG {
    return &CONFIG;
}

pub fn update_config(matches: &ArgMatches) {
    let mut conf = get_config().write().unwrap();
    conf.config_path = Some(PathBuf::from(matches.value_of("config").unwrap()));
    let uconf = read_config(matches);
    conf.src_folder = uconf.src_folder;
    conf.files_to_read = uconf.files_to_read;
    conf.git_username = uconf.git_username;
    conf.git_password = uconf.git_password;
    conf.ssh_askpass = uconf.ssh_askpass;
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
