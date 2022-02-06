#!/bin/sh
#
# Prepare the build environment to speed up builds.
#

# update packages
pacman -Syu --noconfirm

# install required packages
pacman -S --needed --noconfirm cargo git python-toml

# clean up pacman cache
pacman -Sc --noconfirm

# check out the repository
git clone https://github.com/RavuAlHemio/rocketbot.git /rocketbot
cd /rocketbot

# pre-build everything once
cargo build --all-targets || exit 1
cargo build --all-targets --release || exit 1

# the current version of all dependencies should now be prepared

# clean up the repository
cd /
rm -rf /rocketbot
