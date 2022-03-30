mod dl_upd;
mod git_ops;
use clap::{Arg, ArgMatches, Command, Parser};
use dl_upd::{download_repos, update_config, update_directories};
use log::{error, info, warn};
use simple_logger::SimpleLogger;
use tokio;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    match m.subcommand() {
        Some(("update", matches)) => {
            if matches.is_present("update") && matches.is_present("config") {
                info!("Config found, starting update");
                update_config(matches);
                update_directories().await;
            }
        }
        Some(("download", matches)) => {
            if matches.is_present("download") && matches.is_present("config") {
                info!("Config found, starting download");
                update_config(matches);
                download_repos().await;
            }
        }
        _ => error!("Incorrect configuration and flags provided"),
    }

    Ok(())
}
