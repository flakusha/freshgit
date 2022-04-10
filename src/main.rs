mod dl_upd;
mod git_ops;
use clap::{Arg, Command};
use dl_upd::{download_repos, update_directories};
use log::{error, info};
use simple_logger::SimpleLogger;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn main() {
    SimpleLogger::new().init().unwrap();
    let m = Command::new("freshgit - git repositories downloader and updater")
        .author("flakusha, zenflak@gmail.com")
        .version(VERSION)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("CONF")
                .takes_value(true)
                .multiple_values(false)
                .help("Path to configuration .json file")
                .required(true),
        )
        // .arg(
        //     Arg::new("tasks")
        //         .short('t')
        //         .long("tasks")
        //         .value_name("TASKS")
        //         .takes_value(true)
        //         .multiple_values(false)
        //         .help("Amount of repos to process at once")
        //         .required(false),
        // )
        .subcommand(
            Command::new("update")
                .short_flag('u')
                .long_flag("update")
                .about("Updates/fetches folders provided in config file"),
        )
        .subcommand(
            Command::new("download")
                .short_flag('d')
                .long_flag("download")
                .about("Downloads git repositories provided in config file"),
        )
        .get_matches();

    info!("Checking for subcommands, building runtime");
    // Runtime with hardcoded thread number
    // let rt = runtime::Builder::new_multi_thread()
    //     .worker_threads(4)
    //     .max_blocking_threads(4)
    //     .enable_all()
    //     .build()
    //     .unwrap();

    match m.subcommand() {
        Some(("update", _upd)) => {
            info!("Starting repositories update");
            update_directories(m);
        }
        Some(("download", _dwl)) => {
            info!("Starting repositories download");
            download_repos(m);
        }
        _ => {
            error!("Incorrect config is provided");
        }
    }
}
