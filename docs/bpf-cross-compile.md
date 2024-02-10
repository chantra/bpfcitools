
# Cross-compiling BPF

> [!WARNING] don't go crazy in installed foreign arch packages, it may mess some of your system symlinks. Probably better off doing this in a container....

```
XPLATFORM="s390x"
XARCH="s390"
# Set up repo for s390x, it is only available from the `ports` repo.
cat <<EOF >> /etc/apt/sources.list.d/s390x.list
deb [arch=s390x] http://ports.ubuntu.com/ubuntu-ports  mantic main restricted
deb [arch=s390x] http://ports.ubuntu.com/ubuntu-ports  mantic-updates main restricted
EOF
# Add the architecture
sudo dpkg --add-architecture s390x

apt install g{cc,++}-"${XARCH}-linux-gnu" {libelf-dev,libssl-dev,pkgconf}:s390x

KBUILD_OUTPUT_DIR="/tmp/kbuild-${XPLATFORM}"
mkdir "${KBUILD_OUTPUT_DIR}"
cat tools/testing/selftests/bpf/config{,.vm,.${XPLATFORM}} > ${KBUILD_OUTPUT_DIR}/.config

make ARCH="${XARCH}" CROSS_COMPILE="${XPLATFORM}-linux-gnu-" O="${KBUILD_OUTPUT_DIR}"  -j$((4 * $(nproc))) olddefconfig
make ARCH="${XARCH}" CROSS_COMPILE="${XPLATFORM}-linux-gnu-" O="${KBUILD_OUTPUT_DIR}"  -j$((4 * $(nproc))) all
make ARCH="${XARCH}" CROSS_COMPILE="${XPLATFORM}-linux-gnu-" O="${KBUILD_OUTPUT_DIR}"  -j$((4 * $(nproc))) -C tools/testing/selftests/bpf
```

Building selftest with clang:
```
make  ARCH="${XARCH}" CROSS_COMPILE="${XPLATFORM}-linux-gnu-" O="${KBUILD_OUTPUT_DIR}"  -j$((4 * $(nproc))) CLANG=clang-16 LLC=llc-16 LLVM_STRIP=llvm-strip-16 VMLINUX_BTF="${KBUILD_OUTPUT_DIR}/vmlinux" VMLINUX_H= -C tools/testing/selftests/bpf
```

## Generate Ubuntu 23.10 rootfs

In order to run the kernel in a VM we need a rootfs for s390x of the same Ubuntu version to avoid library mismatch.

```
docker2rootfs -R registry-1.docker.io -i s390x/ubuntu -r 23.10 -o /tmp/s390x_rootfs
# chroot and install a few useful packages
cp /etc/resolv.conf /tmp/s390x_rootfs/etc/
sudo chroot  /tmp/s390x_rootfs/
# Within chroot
mount -t devtmpfs -o nosuid,noexec dev /dev
mount -t tmpfs tmpfs /tmp
apt update
DEBIAN_FRONTEND=noninteractive apt-get install -y qemu-guest-agent ethtool keyutils iptables gawk libelf1 zlib1g libssl3
umount /tmp /dev
# exit chroot
exit
```

## Run kernel within chroot

```
vmtest -k "${KBUILD_OUTPUT_DIR}/arch/s390/boot/bzImage" -r /tmp/s390x_rootfs/ -a s390x "uname -m" | cat
```