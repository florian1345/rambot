use crate::config::Config;
use crate::plugin::{Plugin, PluginError, PluginManager};

use rambot_api::communication::{
    BotMessageData,
    ConnectionIntent,
    PluginMessageData,
    Token
};

use std::{process::Child, collections::HashSet, thread::JoinHandle};
use std::convert::TryFrom;
use std::fs;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

fn handle_registration(stream: TcpStream, manager: Arc<Mutex<PluginManager>>) {
    let mut plugin = Plugin::new(stream);
    let plugin_id = manager.lock().unwrap()
        .register_plugin(plugin.clone());
    let conversation_id =
        plugin.send_new(BotMessageData::StartRegistration).unwrap();

    loop {
        match plugin.receive_blocking(conversation_id).unwrap() {
            PluginMessageData::RegisterSource(name) => {
                let successful = manager.lock().unwrap()
                    .register_source(plugin_id, name.clone());

                if successful {
                    log::info!("Registered audio source \"{}\".",
                        name);
                }
                else {
                    log::warn!("Duplicate registration for audio \
                        source {}. Only one will work.", name);
                }
            },
            PluginMessageData::RegistrationFinished => {
                break;
            },
            _ => {} // should not happen
        }
    }
}

fn handle_incoming_connection(mut stream: TcpStream,
        tokens: &Arc<Mutex<HashSet<Token>>>,
        manager: &Arc<Mutex<PluginManager>>) -> (bool, Option<JoinHandle<()>>) {
    stream.set_nonblocking(false).unwrap();
    let intent = match ConnectionIntent::try_from(&mut stream) {
        Ok(i) => i,
        Err(e) => {
            log::warn!("Error receiving connection intent: {}", e);
            return (false, None);
        }
    };

    match intent {
        ConnectionIntent::RegisterPlugin(token) =>
            if tokens.lock().unwrap().contains(&token) {
                let manager = Arc::clone(&manager);
                let join_handle = thread::spawn(
                    move || handle_registration(stream, manager));
                (false, Some(join_handle))
            }
            else {
                log::warn!(
                    "Plugin attempted to register with invalid token.");
                (false, None)
            },
        ConnectionIntent::CloseRegistration(token) => {
            if tokens.lock().unwrap().remove(&token) {
                (tokens.lock().unwrap().is_empty(), None)
            }
            else {
                log::warn!(
                    "Plugin attempted to close registration with invalid \
                    token.");
                (false, None)
            }
        }
    }
}

fn listen(port: u16, tokens: Arc<Mutex<HashSet<Token>>>, abort: Receiver<()>,
        result: Sender<PluginManager>) {
    let mut manager = Arc::new(Mutex::new(PluginManager::new()));
    let mut resolvers = Vec::new();
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    listener.set_nonblocking(true).unwrap();

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let (finished, resolver) =
                    handle_incoming_connection(stream, &tokens, &manager);

                for resolver in resolver {
                    resolvers.push(resolver);
                }

                if finished {
                    break;
                }
            },
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock &&
                        abort.try_recv().is_ok() {
                    break;
                }
            }
        }
    }

    for resolver in resolvers {
        resolver.join().unwrap();
    }

    loop {
        match Arc::try_unwrap(manager) {
            Ok(m) => {
                result.send(m.into_inner().unwrap()).unwrap();
                return
            },
            Err(a) => manager = a
        }
    }
}

fn is_executable(p: &PathBuf) -> bool {
    if !p.is_file() {
        return false;
    }

    let extension = p.extension().and_then(|o| o.to_str());

    if let Some(extension) = extension {
        extension.to_lowercase() == "exe"
    }
    else {
        true
    }
}

fn start_all_plugins(path: &str, port: u16, tokens: Arc<Mutex<HashSet<Token>>>)
        -> Result<Vec<Child>, PluginError> {
    let mut children = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let matches = fs::read_dir(&path)?
                .filter(|e| e.is_ok())
                .map(|e| e.unwrap())
                .map(|e| e.path())
                .filter(is_executable)
                .collect::<Vec<_>>();

            if matches.len() != 1 {
                continue;
            }

            if let Some(s) = &matches[0].as_os_str().to_str() {
                log::info!("Launching executable {} ...", s);
            }
            else {
                log::info!("Launching executable ....");
            }

            let token = Token::new();
            tokens.lock().unwrap().insert(token.clone());
            let child_res = Command::new(&matches[0])
                .current_dir(&path)
                .arg(port.to_string())
                .arg(token.to_string())
                .spawn();

            match child_res {
                Ok(c) => children.push(c),
                Err(e) => {
                    log::error!("Error starting plugin process: {}", e);
                }
            }
        }
    }

    Ok(children)
}

/// Loads all plugins in the plugin directory.
pub fn load(config: &Config) -> Result<PluginManager, PluginError> {
    let port = config.plugin_port();
    let registration_timeout = config.registration_timeout();
    let tokens = Arc::new(Mutex::new(HashSet::new()));
    let lock_token = Token::new();
    tokens.lock().unwrap().insert(lock_token.clone());
    let tokens_clone = Arc::clone(&tokens);
    let (abort_sender, abort_reciever) = mpsc::channel();
    let (result_sender, result_receiver) = mpsc::channel();

    thread::spawn(move ||
        listen(port, tokens_clone, abort_reciever, result_sender));

    log::info!("Loading plugins ...");

    let path = config.plugin_directory();
    let tokens_clone = Arc::clone(&tokens);
    let children = start_all_plugins(path, port, tokens_clone)?;
    tokens.lock().unwrap().remove(&lock_token);
    let mut manager = {
        if let Ok(m) = result_receiver.recv_timeout(registration_timeout) {
            m
        }
        else {
            abort_sender.send(()).unwrap();
            result_receiver.recv().unwrap()
        }
    };

    for child in children {
        manager.register_child(child);
    }

    log::info!("Successfully loaded {} plugins ({} processes).",
        manager.plugins.len(), manager.children.len());
    Ok(manager)
}
