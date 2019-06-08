#!/bin/sh
set -xeu
host=$1

cargo build --target x86_64-unknown-linux-musl  --release
rsync -zarvP ./target/x86_64-unknown-linux-musl/release/hook-listener geb.necrofish.org.uk:bin/
ssh "$host" mkdir -vp .config/systemd/user/ .config/hook-listener
rsync -zarvP srv/hook-listener.service geb.necrofish.org.uk:./.config/systemd/user/hook-listener.service
rsync -zarvP srv/config-env geb.necrofish.org.uk:./.config/hook-listener/env.example
