use anyhow::{anyhow, Result};
use clap::{crate_name, crate_version, App as Clapp, Arg};
use log::info;
use std::{env::set_var, env::var};

use dbus::message::MatchRule;
use dbus::nonblock::stdintf::org_freedesktop_dbus::Properties;
use dbus::nonblock::Proxy;
use dbus::Path;
use dbus_tokio::connection;
use futures::channel;
use tokio::stream::StreamExt;

use dbus::nonblock::MsgMatch;

use std::collections::HashMap;
use std::time::Duration;
use std::process::Command;

mod config;
use config::{Config, ConnectionConfig};

const CONFIG_FILE_ENV: &str = "NM_DBUS_CONNECTION_NOTIFIER_CONFIG";
// const DBUS_NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DBUS_NM_OBJECT_NAME: &str = "org.freedesktop.NetworkManager";
const DEFAULT_TIMEOUT: u64 = 1000;

// `nmcli c` to get connections uuid

#[derive(Debug, PartialEq)]
enum ActiveConnectionAction {
    Up,
    Down,
}

type DbusPath = Path<'static>;
type DbusPathMessage = (dbus::message::Message, (DbusPath,));
type DbusPathChannel = channel::mpsc::UnboundedReceiver<DbusPathMessage>;

struct Watcher {
    // Dbus Connection
    conn: std::sync::Arc<dbus::nonblock::SyncConnection>,
    // Map connection uuid to their config
    name_config_map: HashMap<String, ConnectionConfig>,
    // Map nm devices to their uuid
    active_devices_map: HashMap<DbusPath, String>,
    // Signal token
    conn_added_sig: MsgMatch,
    conn_removed_sig: MsgMatch,
    // Event streams
    conn_added_stream: DbusPathChannel,
    conn_removed_stream: DbusPathChannel,
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

        let (conn_added_sig, conn_added_stream): (MsgMatch, DbusPathChannel) = conn
            .clone()
            .add_match(MatchRule::new_signal(
                "org.freedesktop.DBus.ObjectManager",
                "InterfacesAdded",
            ))
            .await?
            .stream();
        let (conn_removed_sig, conn_removed_stream): (MsgMatch, DbusPathChannel) = conn
            .clone()
            .add_match(MatchRule::new_signal(
                "org.freedesktop.DBus.ObjectManager",
                "InterfacesRemoved",
            ))
            .await?
            .stream();

        Ok(Self {
            conn: conn.clone(),
            name_config_map: config.connections,
            active_devices_map: HashMap::new(),
            conn_added_sig,
            conn_removed_sig,
            conn_added_stream,
            conn_removed_stream,
        })
    }

    async fn run(&mut self) -> Result<()> {
        // TODO Call for all already active connections
        //         let proxy = Proxy::new(
        //             DBUS_NM_OBJECT_NAME,
        //             DBUS_NM_PATH,
        //             Duration::from_millis(DEFAULT_TIMEOUT),
        //             self.conn.clone(),
        //         );
        //         let (present_devices,): (Vec<DbusPath>,) = proxy
        //             .method_call(DBUS_NM_OBJECT_NAME, "GetAllDevices", ())
        //             .await?;
        //         for device in present_devices {
        //             // Consider current present device up
        //             self.active_connection_mutation(ActiveConnectionAction::Up, device).await;
        //         }

        // Run forever.
        // TODO setup a unix signal handler and use a channel to close soft softly
        loop {
            let (act, msg) = tokio::select! {
                Some(msg) =  self.conn_added_stream.next() => {
                    (ActiveConnectionAction::Up, msg)
                },
                Some(msg) = self.conn_removed_stream.next() => {
                    (ActiveConnectionAction::Down, msg)
                },
                else => break,
            };
            let active_connection = (msg.1).0;
            // Consider only active connections
            if active_connection
                .as_cstr()
                .to_str()
                .unwrap()
                .to_owned()
                .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
            {
                self.active_connection_mutation(act, active_connection)
                    .await;
            }
        }

        self.teardown().await?;
        Ok(())
    }

    async fn teardown(&mut self) -> Result<()> {
        self.conn.remove_match(self.conn_added_sig.token()).await?;
        self.conn
            .remove_match(self.conn_removed_sig.token())
            .await?;
        Ok(())
    }

    async fn get_connection_uuid(&self, active_connection: DbusPath) -> Option<String> {
        let conn_proxy = Proxy::new(
            DBUS_NM_OBJECT_NAME,
            active_connection,
            Duration::from_millis(DEFAULT_TIMEOUT),
            self.conn.clone(),
        );
        match conn_proxy
            .get::<String>("org.freedesktop.NetworkManager.Connection.Active", "Uuid")
            .await
        {
            Ok(r) => Some(r),
            Err(_e) => None,
        }
    }

    async fn active_connection_mutation(
        &mut self,
        action: ActiveConnectionAction,
        active_connection: DbusPath,
    ) {
        match action {
            ActiveConnectionAction::Up => {
                if let Some(uuid) = self.get_connection_uuid(active_connection.clone()).await {
                    self.active_devices_map
                        .entry(active_connection)
                        .or_insert_with(|| uuid.clone());
                    self.call_user_defined_script(&uuid, ActiveConnectionAction::Up)
                        .await;
                }
            }
            ActiveConnectionAction::Down => {
                if let Some(uuid) = self
                    .active_devices_map
                    .remove_entry(&active_connection)
                    .map(|e| e.1)
                {
                    self.call_user_defined_script(&uuid, ActiveConnectionAction::Down)
                        .await;
                }
            }
        };
    }

    async fn call_user_defined_script(&self, uuid: &str, action: ActiveConnectionAction) {
        if let Some(conn_params) = self.name_config_map.get(uuid) {
            let script = match action {
                ActiveConnectionAction::Up => &conn_params.up_script,
                ActiveConnectionAction::Down => &conn_params.down_script,
            };
            info!("{} {:?}", conn_params.name, action);
            Command::new("/bin/sh").arg("-c").arg(script).env("CONNECTION_NAME", &conn_params.name).env("CONNECTION_CONTEXT", &conn_params.context).spawn().expect("Command failed.");
        }
    }
}

#[tokio::main]
pub async fn main() -> Result<()> {
    // Load the CLI parameters from the yaml file
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

    // Retrieve the config file path
    let config_filename = match matches.value_of("config") {
        Some(c) => Ok(c.to_owned()),
        None => var(CONFIG_FILE_ENV).map_err(|_e| {
            anyhow!(
                "No config provided either with -c or {} environment variable",
                CONFIG_FILE_ENV
            )
        }),
    }?;

    // Parse the configuration
    let config = Config::from_file(&config_filename)?;

    // Set the logging verbosity
    set_var("RUST_LOG", "info");
    // Initialize the logger
    env_logger::init();

    let mut watcher = Watcher::from_config(config).await?;
    info!("Watching");
    watcher.run().await?;
    Ok(())
}
