use crate::plugin::{Plugin, PluginError, PluginManager};

use rambot_api::communication::{BotMessageData, PluginMessageData};

use std::fs;
use std::net::TcpListener;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// TODO make configurable
const PORT: u16 = 46085;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(10);
const PLUGIN_DIRECTORY: &str = "plugins";

fn listen() -> PluginManager {
    let mut manager = Arc::new(Mutex::new(PluginManager::new()));
    let mut resolvers = Vec::new();
    let listener = TcpListener::bind(("127.0.0.1", PORT)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut last_action = Instant::now();

    while (Instant::now() - last_action) < REGISTRATION_TIMEOUT {
        while let Ok((stream, _)) = listener.accept() {
            last_action = Instant::now();
            let manager = Arc::clone(&manager);
            resolvers.push(thread::spawn(move || {
                let mut plugin = match Plugin::new(stream) {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("Could not load plugin: {}", e);
                        return;
                    }
                };
                let plugin_id = manager.lock().unwrap()
                    .register_plugin(plugin.clone());
                let conversation_id =
                    plugin.send_new(BotMessageData::StartRegistration)
                        .unwrap();

                loop {
                    match plugin.receive_blocking(conversation_id) {
                        PluginMessageData::RegisterSource(name) => {
                            manager.lock().unwrap()
                                .register_source(plugin_id, name);
                        },
                        PluginMessageData::RegistrationFinished => {
                            break;
                        },
                        _ => {} // should not happen
                    }
                }
            }))
        }

        thread::sleep(POLL_INTERVAL);
    }

    for resolver in resolvers {
        resolver.join().unwrap();
    }

    loop {
        match Arc::try_unwrap(manager) {
            Ok(m) => return m.into_inner().unwrap(),
            Err(a) => manager = a
        }
    }
}

/// Loads all plugins in the plugin directory.
pub fn load() -> Result<PluginManager, PluginError> {
    let listener = thread::spawn(listen);
    let mut children = Vec::new();

    log::info!("Loading plugins ...");

    for entry in fs::read_dir(PLUGIN_DIRECTORY)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let matches = fs::read_dir(&path)?
                .filter(|e| e.is_ok())
                .map(|e| e.unwrap())
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.ends_with(".exe"))
                .collect::<Vec<_>>();

            if matches.len() != 1 {
                continue;
            }

            let child_res = Command::new(&matches[0])
                .current_dir(&path)
                .spawn();

            match child_res {
                Ok(c) => children.push(c),
                Err(e) => {
                    log::error!("Error starting plugin process: {}", e);
                }
            }
        }
    }

    let mut manager = listener.join().unwrap();

    for child in children {
        manager.register_child(child);
    }

    log::info!("Successfully loaded {} plugins ({} processes).",
        manager.plugins.len(), manager.children.len());
    Ok(manager)
}
