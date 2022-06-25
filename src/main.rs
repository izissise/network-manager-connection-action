use anyhow::{anyhow, Result};
use clap::{crate_name, crate_version, App as Clapp, Arg};
use log::*;
use std::{
    collections::HashMap, env::set_var, env::var, pin::Pin, process::Stdio, sync::Arc,
    time::Duration,
};

use dbus::{
    message::MatchRule,
    nonblock::{stdintf::org_freedesktop_dbus::Properties, MsgMatch, Proxy},
    Path,
};
use dbus_tokio::connection;
use futures::{channel::mpsc::channel, prelude::*, stream::SelectAll};

use tokio::process::{Child, Command};

mod config;
use config::{Config, ConnectionConfig};

const CONFIG_FILE_ENV: &str = "NM_DBUS_CONNECTION_ACTION_CONFIG";
// const DBUS_NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DBUS_NM_ACTIVE_CONNECTION_PATH: &str = "/org/freedesktop/NetworkManager/ActiveConnection/";
const DBUS_NM_OBJECT_NAME: &str = "org.freedesktop.NetworkManager";
const DBUS_DEFAULT_TIMEOUT: u64 = 1000;

// `nmcli c` to get connections uuid

/// Event on a watched connection
#[derive(Debug, PartialEq, Clone)]
enum ConnectionEvent {
    Up,
    Down,
}

type DbusPath = Path<'static>;
type DbusPathMessage = (dbus::message::Message, (DbusPath,));
type IfaceEvStream =
    SelectAll<Pin<Box<(dyn Stream<Item = (ConnectionEvent, DbusPathMessage)> + Send)>>>;

/// Watch for dbus events and execute user's scripts
struct Watcher {
    /// Dbus Connection
    conn: Arc<dbus::nonblock::SyncConnection>,
    /// Map connection uuid to their config
    user_config_map: HashMap<String, ConnectionConfig>,
    /// Map nm devices to their (id, uuid)
    up_map: HashMap<DbusPath, (String, String)>,
    /// Stop watching signal events token
    iface_add_signal: MsgMatch,
    /// Stop watching signal events token
    iface_del_signal: MsgMatch,
    /// Event stream
    iface_ev_stream: IfaceEvStream,
}

impl Watcher {
    async fn from_config(config: Config) -> Result<Self> {
        // First open up a connection to the system bus.
        let (resource, conn) = connection::new_system_sync()?;

        // The resource is a task that should be spawned onto a tokio compatible
        // reactor ASAP. If the resource ever finishes, you lost connection to D-Bus.
        tokio::spawn(async {
            let err = resource.await;
            panic!("Lost connection to D-Bus: {}", err);
        });

        // Create dbus InterfacesAdded event stream
        let (iface_add_signal, iface_add_stream) = conn
            .clone()
            .add_match(MatchRule::new_signal(
                "org.freedesktop.DBus.ObjectManager",
                "InterfacesAdded",
            ))
            .await?
            .stream();

        // Create dbus InterfacesRemoved event stream
        let (iface_del_signal, iface_del_stream) = conn
            .clone()
            .add_match(MatchRule::new_signal(
                "org.freedesktop.DBus.ObjectManager",
                "InterfacesRemoved",
            ))
            .await?
            .stream();

        // Merge event stream into one
        let add_ev = iface_add_stream
            .map(|msg| (ConnectionEvent::Up, msg))
            .boxed();
        let del_ev = iface_del_stream
            .map(|msg| (ConnectionEvent::Down, msg))
            .boxed();
        let iface_ev_stream = stream::select_all(vec![add_ev, del_ev]);

        let user_config_map = config.connections;
        let up_map = HashMap::new();

        Ok(Self {
            conn,
            user_config_map,
            up_map,
            iface_add_signal,
            iface_del_signal,
            iface_ev_stream,
        })
    }

    async fn run(&mut self) -> Result<()> {
        // TODO Call all already active connections
        // TODO setup a unix signal handler and use a channel to quit service softly

        // Keep track of spawned processes
        let (mut child_process_in, child_process_out) = channel(2);
        let mut process_wait = child_process_out.map(|mut p: Child| async move { p.wait().await });
        loop {
            tokio::select! {
                Some((act, msg)) = self.iface_ev_stream.next() => {
                    let connection = (msg.1).0;
                    // Consider only active connections
                    if connection.starts_with(DBUS_NM_ACTIVE_CONNECTION_PATH) {
                        // if the event correspond to something in up_map
                        // we call associated command
                        if let Some((conn_id, conn_uuid)) = self.connection_event(act.clone(), connection).await {
                            if let Some(child) = self.run_conn_cmd(&conn_id, &conn_uuid, &act).await {
                                child_process_in.send(child).await?;
                            }
                        }
                    }
                },
                Some(child) = process_wait.next() => {
                    let ex_code = child.await?;
                    if !ex_code.success() {
                        match ex_code.code() {
                            Some(code) => warn!("Command exited with status {}", code),
                            None => warn!("Command terminated by signal"),
                        };
                    }
                },
                else => break,
            };
        }

        // Clean before exit
        self.teardown().await?;
        Ok(())
    }

    async fn teardown(&mut self) -> Result<()> {
        info!("Tearing down dbus event streams");
        self.conn
            .remove_match(self.iface_add_signal.token())
            .await?;
        self.conn
            .remove_match(self.iface_del_signal.token())
            .await?;
        Ok(())
    }

    async fn connection_uuid(&self, act_conn: &DbusPath) -> Option<(String, String)> {
        let dbus_endpoint = "org.freedesktop.NetworkManager.Connection.Active";
        let conn_proxy = Proxy::new(
            DBUS_NM_OBJECT_NAME,
            act_conn,
            Duration::from_millis(DBUS_DEFAULT_TIMEOUT),
            self.conn.clone(),
        );

        match (
            conn_proxy
                .get::<String>(dbus_endpoint, "Id")
                .await
                .ok_or_log_err("connection_Id:"),
            conn_proxy
                .get::<String>(dbus_endpoint, "Uuid")
                .await
                .ok_or_log_err("connection_uuid:"),
        ) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    }

    async fn connection_event(
        &mut self,
        action: ConnectionEvent,
        nm_conn: DbusPath,
    ) -> Option<(String, String)> {
        match action {
            // Iface is up
            ConnectionEvent::Up => self.connection_uuid(&nm_conn).await.map(|(id, uuid)| {
                self.up_map
                    .entry(nm_conn)
                    .or_insert_with(|| (id.clone(), uuid.clone()));
                (id, uuid)
            }),
            // Iface is down
            ConnectionEvent::Down => self.up_map.remove_entry(&nm_conn).map(|e| e.1),
        }
    }

    fn get_conn_params(&self, id: &str, uuid: &str) -> Option<&ConnectionConfig> {
        if let Some(conn_params) = self.user_config_map.get(uuid) {
            Some(conn_params)
        } else if let Some(conn_params) = self.user_config_map.get(id) {
            Some(conn_params)
        } else {
            None
        }
    }

    async fn run_conn_cmd(&self, id: &str, uuid: &str, action: &ConnectionEvent) -> Option<Child> {
        if let Some(conn_params) = self.get_conn_params(id, uuid) {
            let cmd = match action {
                ConnectionEvent::Up => &conn_params.up_script,
                ConnectionEvent::Down => &conn_params.down_script,
            };
            info!("{} {:?}", conn_params.name, action);
            Command::new("/bin/sh")
                .arg("-c")
                .arg(cmd)
                .env("CONNECTION_NAME", &conn_params.name)
                .env("CONNECTION_CONTEXT", &conn_params.context)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .ok_or_log_err("Command failed:")
        } else {
            None
        }
    }
}

#[tokio::main]
pub async fn main() -> Result<()> {
    // Load CLI parameters from yaml file
    let cli = Clapp::new(crate_name!()).version(crate_version!()).arg(
        Arg::with_name("config")
            .short("c")
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file")
            .takes_value(true),
    );
    let _app_name = cli.get_name().to_owned();
    let matches = cli.get_matches();

    // Retrieve config file path
    let config_filename = match matches.value_of("config") {
        Some(c) => Ok(c.to_owned()),
        None => var(CONFIG_FILE_ENV).map_err(|_e| {
            anyhow!(
                "No config provided either with -c or {} environment variable",
                CONFIG_FILE_ENV
            )
        }),
    }?;

    // Parse configuration
    let config = Config::from_file(&config_filename)?;

    // Set logging verbosity
    set_var("RUST_LOG", "info");
    // Initialize logger
    env_logger::init();

    let mut watcher = Watcher::from_config(config).await?;
    info!("Watching for NetworkManager events");
    watcher.run().await?;
    Ok(())
}

/// Log errors on Result
pub trait ResultOkLogErrExt<T> {
    fn ok_or_log_err(self, msg: &str) -> Option<T>;
}

impl<T, E> ResultOkLogErrExt<T> for Result<T, E>
where
    E: ::std::fmt::Display,
{
    fn ok_or_log_err(self, msg: &str) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                error!("{}: {}", msg, e);
                None
            }
        }
    }
}
