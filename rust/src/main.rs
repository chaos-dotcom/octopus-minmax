//! `src/main.py` - the bot thread and the web server thread.

mod account_info;
mod account_manager;
mod apprise;
mod bot_orchestrator;
mod clock;
mod comparison_engine;
mod config;
mod errors;
mod home_assistant_client;
mod http;
mod jsonutil;
mod logger;
mod notification_service;
mod pyhash;
mod pyrepr;
mod queries;
mod query_service;
mod session;
mod tariff;
mod templates;
mod urlcode;
mod web_server;

use std::thread;

fn main() {
    config::initialise();
    logger::initialise();

    let mut orchestrator = bot_orchestrator::BotOrchestrator::new();

    // The reference starts the bot thread first but the web thread reaches its
    // startup log line first (the bot thread parses notification URLs before it
    // logs anything).  Starting the web thread first reproduces that order
    // deterministically; the stdout lines keep the reference's order.
    println!("Starting bot thread...");
    let web_thread = thread::Builder::new()
        .name("WebThread".to_string())
        .spawn(web_server::run_server);
    let web_thread = match web_thread {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("could not start the web server thread: {}", error);
            std::process::exit(1);
        }
    };

    let bot_thread = thread::Builder::new()
        .name("BotThread".to_string())
        .spawn(move || orchestrator.start());
    let bot_thread = match bot_thread {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("could not start the bot thread: {}", error);
            std::process::exit(1);
        }
    };

    println!("Starting web server thread...");

    let _ = bot_thread.join();
    let _ = web_thread.join();
}
