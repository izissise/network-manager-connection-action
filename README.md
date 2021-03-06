# network-manager-connection-action

Listen connections/deconnections using network-manager's dbus interface and execute specified commands without needing root access

## Use case

One possible use case is adding auto ssh canonicalization for company domains when connected to company vpn, this usage can be found in `example` directory.

## Build and Install

```
cargo build --release
cp target/release/network-manager-connection-action /usr/bin/network-manager-connection-action
```


## Config

The systemd example put the config file here `$HOME/.config/network_manager_connection_actionrc`

The config contains network manager uuid that correspond to a connection.

You can find connections's uuid using `nmcli c`

For each connections one can pass a name and a context that will be available in the scripts

## Systemd autostart

Put the unit file here
```
$HOME/.config/systemd/user/network-manager-connection-action.service
```

Run and enable
```
systemctl --user daemon-reload
systemctl --user start network-manager-connection-action.service
systemctl --user enable network-manager-connection-action.service
journalctl --user -fu network-manager-connection-action.service
```

## Future

Use systemd varlink interface

The code and functionality could be improved in many ways, don't hesitate to open merge requests :)

