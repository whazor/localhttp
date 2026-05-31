# Service Managers

Run `localhttp serve` as an operating-system service when you want the default
listeners on ports `80` and `443`. Those ports normally require elevated
privileges, so the service manager owns the privileged daemon and normal user
shells only run route and certificate commands.

Install `localhttp` somewhere stable first, such as:

```sh
nix build path:.
sudo install -m 0755 result/bin/localhttp /usr/local/bin/localhttp
```

If you install the binary somewhere else, update the `ExecStart` or
`ProgramArguments` path in the template before loading it.

## systemd

Template:

```text
contrib/systemd/localhttp.service
```

Install and start:

```sh
sudo install -m 0644 contrib/systemd/localhttp.service /etc/systemd/system/localhttp.service
sudo systemctl daemon-reload
sudo systemctl enable --now localhttp.service
```

Inspect:

```sh
systemctl status localhttp.service
journalctl -u localhttp.service -f
```

Stop and unload:

```sh
sudo systemctl disable --now localhttp.service
sudo rm /etc/systemd/system/localhttp.service
sudo systemctl daemon-reload
```

## launchd

Template:

```text
contrib/launchd/dev.localhttp.plist
```

Install and start:

```sh
sudo install -m 0644 contrib/launchd/dev.localhttp.plist /Library/LaunchDaemons/dev.localhttp.plist
sudo chown root:wheel /Library/LaunchDaemons/dev.localhttp.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/dev.localhttp.plist
sudo launchctl enable system/dev.localhttp
sudo launchctl kickstart -k system/dev.localhttp
```

Inspect:

```sh
launchctl print system/dev.localhttp
tail -f /tmp/localhttp.launchd.err.log
```

Stop and unload:

```sh
sudo launchctl bootout system /Library/LaunchDaemons/dev.localhttp.plist
sudo rm /Library/LaunchDaemons/dev.localhttp.plist
```

## After The Service Starts

Generate certificates and register apps from a normal shell:

```sh
localhttp certs
port="$(localhttp test-app)"
my-test-app --port "$port"
```

The daemon keeps routes in memory, so apps should register again after the
service restarts.
