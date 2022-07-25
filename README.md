# Rambot

The **"Rambot Augmentable Music BOT"** for Discord (Rambot) is a music bot written in Rust with the main design goal of extensibility.
It achieves this goal by dynamically loading plugins that offer various functionality, on top of the core feature set provided by the bot itself.
Some default plugins are provided along with the bot in this repository.

## Features

The bot itself implements the following features.

* Playback of audio on multiple **layers** simultaneously
* Audio **effects**, which can be put on individual layers
* **Playlists**
* **Adapters**, which can alter playlists (such as shuffling or looping)
* **Sound boards** with configurable buttons that execute user-defined commands

In addition, the default plugins offer the following functionality.

* Playback of local `*.wav`, `*.ogg`, and `*.mp3` files by `plugin-wave`, `plugin-ogg`, and `plugin-mp3` respectively
* Playing all music inside a directory as a playlist by `plugin-folder-list`
* JSON-playlists by `plugin-json-list`
* Looping of individual songs and playlists by `plugin-loop`
* Shuffling of playlists by `plugin-shuffle`
* An effect controlling the volume of individual layers by `plugin-volume`
* Various audio filters in `plugin-filters`

## Dependencies

Rambot uses [Songbird](https://github.com/serenity-rs/songbird), which requires Opus.

> Opus - Audio codec that Discord uses.
> If you are on Windows and you are using the MSVC toolchain, a prebuilt DLL is provided for you, you do not have to do anything.
> On other platforms, you will have to install it.
> You can install the library with apt install libopus-dev on Ubuntu or pacman -S opus on Arch Linux.
> If you do not have it installed it will be built for you.
> However, you will need a C compiler and the GNU autotools installed.
> Again, these can be installed with apt install build-essential autoconf automake libtool m4 on Ubuntu or pacman -S base-devel on Arch Linux.
> \- [Songbird Readme](https://github.com/serenity-rs/songbird)

## Repository structure

The repository constitutes a Cargo namespace with various crates.

* `rambot` is the core bot executable.
* `rambot-api` is a library crate which defines the interface against which plugins are programmed.
* `rambot-proc-macro` defines procedural macros specifically for the `rambot` crate.
* `plugin-commons` implements and offers some functionality common to multiple default plugins.
* `plugin-*` are default plugins that implement some basic functionality.

## Contributions

If you find a bug, I would be very happy if you could open an issue about it.
Feature requests are also always welcome, however I can make no guarantee about the speed at which they are processed.
If you want to contribute yourself, feel free to open a pull request, however note that all contributions will be licensed under the license provided in this repository.
