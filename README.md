# PennFat Viewer

PFView is a simple TUI viewer for PennFat files.

[![build](https://github.com/AnirudhKonduru/pfview/actions/workflows/build.yml/badge.svg)](https://github.com/AnirudhKonduru/pfview/actions/workflows/build.yml)

## Demo

### Viewing a block as a directory block

![Viewing a block as a directory block](./demo/dir-mode.png)

### Viewing a block as a raw file block

![Vieing a block as a raw file block](./demo/raw-mode.png)

## Why?

`pfview` make's it easier to debug the PennOS project's PennFAT filesystem.
The TUI's view updates automatically as the filesystem is modified, so you may have this running on another terminal as you test your program's (pennfat, pennos, or any other) file system operations, and use it to examine the state of the FAT table or any of the data blocks at any time.

## Why, really?

[Yak shaving.](https://seths.blog/2005/03/dont_shave_that/)

## Usage

First, build the project:

```sh
cargo build --release # builds the binary in ./target/release/pfview
```

You may also download the binary from the [releases page](https://github.com/AnirudhKonduru/pfview/releases)
instead.

Then, run it with the path to the PennFat image as the first argument:

```sh
cargo run ./path/to/pennfat.img
```

or if you have the binary:

```sh
./pfview ./path/to/pennfat.img
```
