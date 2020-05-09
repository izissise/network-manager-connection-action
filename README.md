# network-manager-connection-notifier

Listen for connections and deconnections using network-maanager dbus and execute specified commands

No root needed

## Use case

One possible use case is adding auto ssh canonicalization for company domains when connected to company vpn

## Build

```
cargo build --release
cp target/release/network-manager-connection-notifier /usr/bin/network-manager-connection-notifier
```


## Config

The systemd example put the config file here `$HOME/.config/network_manager_connection_notifierrc`

The config contains network manager uuid that correspond to a connection.

You can find uuid using `nmcli c`

For each connections one can pass a name and a context that will be available in the scripts

## Systemd

Put the unit file here
```
$HOME/.config/systemd/user/network-manager-connection-notifier.service
```

Run and enable
```
systemctl --user daemon-reload
systemctl --user start network-manager-connection-notifier.service
systemctl --user enable network-manager-connection-notifier.service
journalctl --user -fu network-manager-connection-notifier.service
```

## Future

The code and functionality could be improved in many ways, don't hesitate to open a merge request :)

