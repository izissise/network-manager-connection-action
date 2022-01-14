![Crates.io](https://img.shields.io/crates/v/network-manager-connection-action)

# network-manager-connection-action

Listen connections/deconnections using network-manager's dbus interface and execute specified commands without the need of root access

## Use case

One possible use case is adding automatic [ssh canonicalization](https://dotfiles.tnetconsulting.net/articles/2016/0109/ssh-canonicalization.html) for company domains when connecting to company VPN, this usage can be found in `example` directory.

## Example run

Output when configured to run with canonicalization on a vpn connection and connecting/disconnecting from NetworkManager:
```
<user>$ network-manager-connection-action -c example/config.toml
[2021-05-02T13:51:45Z INFO] Watching for NetworkManager events
[2021-05-02T13:53:41Z INFO] Entreprise VPN Up
CanonicalDomains public.entreprise.com internal.entreprise.com anotherdomains.fromvpn
[2021-05-02T13:56:21Z INFO] Entreprise VPN Down
CanonicalDomains public.entreprise.com
```

## Build and Install

```
cargo build --release
cp target/release/network-manager-connection-action /usr/bin/network-manager-connection-action
```

## Config

You need the UUID of networks you want to watch

You can find connections's UUID using `nmcli c`

Create a config file, example `$HOME/.config/network_manager_connection_actionrc`

Config contains network manager uuid that correspond to an existing connection.

For each connection's config you can choose a command and a context, see examples

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

At start the program could query already connected connections and apply configuration

The code and functionality could be improved in many ways, don't hesitate to open merge requests :)

