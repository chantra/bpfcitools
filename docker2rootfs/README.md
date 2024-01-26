# docker2rootfs

A tool to download a docker image from a registry and convert it to a rootfs without the need to install and run docker.

## Build

```
cargo build
```

## Install
```
cargo install --path .
```

## Usage

```
$ docker2rootfs --image kernel-patches/runner -r main-s390x -o /tmp/main-s390x-chantra
Downloading 14 layer(s)
Downloaded 14 layer(s)
Unpacking layers to /tmp/main-s390x-chantra
```

The rootfs is available in `/tmp/main-s390x-chantra`
