# Running BPF tests in BPF CI environment


[BPF CI](https://github.com/kernel-patches/bpf) builds and runs test for every series that make it to [Patchwork netdevbpf project with BPF delegate](https://patchwork.kernel.org/project/netdevbpf/list/?series=&submitter=&state=&q=&archive=&delegate=121173).

Github (GH) Actions are triggered, build the kernel and BPF selftests and store them in GH artifacts. Those same artifacts are then used by the test jobs to boot a VM with that specific kernel and then run the BPF selftests.

At times, test are failing and it is not straightforward from the logs/code to understand what goes wrong. In such case, it can become useful to access the environment in which those tests ran, be able to run commands manually, reproduce the test step-by-step...

Because both the build artifacts, and the CI builder images is accessible post-mortem, we can grab them and run them locally to poke at the system under test.

## Tooling/Setup

We are going to run VMs... so we need to have `qemu` friends installed on the local system.

The steps below are assuming a modern Ubuntu distro. Adapt this to your preferred distro:

```
apt install -y qemu-system-{x86,arm,s390x} qemu-user-static 
```

While technically this should be enough to spin up our VM.... We are going to use a few more tools to help us set up and run our environment. Follow their relative "install" procedures:

* [GH CLI](https://cli.github.com/), this is used to be able to download the build artifacts (e.g kernel and selftests)
* [danobi/vmtest](https://github.com/danobi/vmtest), used to run the tests against the kernel that was built
* [docker2rootfs](../docker2rootfs/), used to download and extract a rootfs from the CI runners' docker images.



## Example:

Assuming we want to test against the artifacts built in [run 7702868911](https://github.com/kernel-patches/bpf/actions/runs/7702868911), we can access the list of artifact under the [artifact anchor](https://github.com/kernel-patches/bpf/actions/runs/7702868911#artifacts).

Create a working directory:
```
mkdir /tmp/7702868911 && cd /tmp/7702868911
```

From there, supposing we want to download the kernel built for s390x with gcc:
```
gh run -R kernel-patches/bpf download 7702868911 -n vmlinux-s390x-gcc -D /tmp/7702868911
```

Then untar this into /tmp/7702868911

```
tar -I zstd -C /tmp/7702868911 -xvf /tmp/7702868911/vmlinux-s390x-gcc.tar.zst
```

Get the rootfs from the runners (optional if you already have the rootfs somewhere):
```
$ docker2rootfs --image kernel-patches/runner -r main-s390x -o /tmp/7702868911/main-s390x
[2024-01-31T21:01:27Z INFO  docker2rootfs] Downloading 14 layer(s)
[2024-01-31T21:01:27Z INFO  docker2rootfs] Downloaded 14 layer(s)
[2024-01-31T21:01:27Z INFO  docker2rootfs] Unpacking layers to /tmp/7702868911/main-s390x
```

Now boot the VM using the s390x kernel (-k) and rootfs (-r), because we work cross-architecture, we also need to specific the target architecture (-a). `vmtest` will mount your current directory under `/mnt/vmtest`.

For instance, to list all test_progs tests:

```
vmtest -k kbuild-output/arch/s390/boot/bzImage -r main-s390x -a s390x "cd /mnt/vmtest/selftests/bpf && ./test_progs -l"
```

To run `assign_reuse` test:

```
$ vmtest -k kbuild-output/arch/s390/boot/bzImage -r main-s390x -a s390x "cd /mnt/vmtest/selftests/bpf && ./test_progs -t assign_reuse"
=> bzImage
===> Booting
===> Setting up VM
===> Running command
root@(none):/# bpf_testmod: loading out-of-tree module taints kernel.
bpf_testmod: module verification failed: signature and/or required key missing - tainting kernel
#4/1     assign_reuse/tcpv4:OK
#4/2     assign_reuse/tcpv6:OK
#4/3     assign_reuse/udpv4:OK
#4/4     assign_reuse/udpv6:OK
#4       assign_reuse:OK
Summary: 1/4 PASSED, 0 SKIPPED, 0 FAILED
```

### Getting a prompt inside the VM

At times, just re-running the test is not enough to get a good understanding of what is failing. In such case, being able to run commands inside the VM can be valuable.

When passing `-` as a command to `vmtest`, you will be dropped into a shell.

> [!WARNING]
> This is currently pending on https://github.com/danobi/vmtest/pull/58 being release. For now you will need to build `vmtest` from master.

```
vmtest -k kbuild-output/arch/s390/boot/bzImage -r main-s390x -a s390x -
```

### Installing more tools inside the rootfs

> [!NOTE]
> Doing this cross-platform relies on `binfmt`, you get this for free with `qemu-user-static` package on Ubuntu.

The BPF CI runner images come with some pre-installed binaries, but don't necessarily have all the tools you could ever need.
Because we are running off a plain rootfs, we can `chroot` into it from the host and install packages with `apt`. Even cross-platform, here is an example from am x86_64 host:

```
# Make sure DNS works within the chroot
$ cp /etc/resolv.conf /tmp/7702868911/main-s390x/etc/
$ sudo chroot  /tmp/7702868911/main-s390x/
# apt-get install -y strace
Reading package lists... Done
Building dependency tree
Reading state information... Done
The following NEW packages will be installed:
  strace
0 upgraded, 1 newly installed, 0 to remove and 6 not upgraded.
Need to get 306 kB of archives.
After this operation, 1373 kB of additional disk space will be used.
Get:1 http://ports.ubuntu.com/ubuntu-ports focal-updates/main s390x strace s390x 5.5-3ubuntu1 [306 kB]
Fetched 306 kB in 1s (247 kB/s)
perl: warning: Setting locale failed.
perl: warning: Please check that your locale settings:
        LANGUAGE = (unset),
        LC_ALL = (unset),
        LC_TERMINAL = "iTerm2",
        LANG = "en_US.UTF-8"
    are supported and installed on your system.
perl: warning: Falling back to the standard locale ("C").
debconf: delaying package configuration, since apt-utils is not installed
E: Can not write log (Is /dev/pts mounted?) - posix_openpt (2: No such file or directory)
Selecting previously unselected package strace.
(Reading database ... 39499 files and directories currently installed.)
Preparing to unpack .../strace_5.5-3ubuntu1_s390x.deb ...
Unpacking strace (5.5-3ubuntu1) ...
Setting up strace (5.5-3ubuntu1) ...
```

`strace` will then be available in the guest rootfs. And because we mount the rootfs in the guest, you can install packages in the chroot and they will be made available in the guest immediately.

## Tips, Tricks, Notes, Caveats


### Streaming output to stdout

By default `vmtest` will print the output in a small viewport it is detects a tty, and will dump all text when done. If you want to see the text scrolling, pipe `vmtest` into `cat`, e.g:

```
vmtest -k kbuild-output/arch/s390/boot/bzImage -r main-s390x -a s390x "cd /mnt/vmtest/selftests/bpf && ./test_progs -t assign_reuse" | cat
```

### Setting VM environment for tests

We have a lot of different tests that run and some of them assume the VM to be set in a specific way.

If you are going to run all tests, you likely need to make sure those commands are run:
```
/bin/mount bpffs /sys/fs/bpf -t bpf && \
  ip link set lo up
```

So, if you wanted to run all tests from `test_progs` in a one-liner:
```
vmtest -k kbuild-output/arch/s390/boot/bzImage -r main-s390x -a s390x "/bin/mount bpffs /sys/fs/bpf -t bpf && \
  ip link set lo up && \
  cd /mnt/vmtest/selftests/bpf && \
  ./test_progs -l"
```

### Ctrl-C/Ctrl-Z are not propagated to the VM in interactive mode

Yes... this is a current limitation of vmtest interactive mode. There is likely a solution to this but it needs to be implemented. For now we need to work around it.

### Why not using ${OTHER_QEMU_WRAPPER}

There is plenty of them and likely people have their own preferences. `vmtest` was appealing because:
- it is simple and straightforward to use as a one liner
- was solving some of the problems we had with the previous [rootfs approach from BPF CI](https://github.com/libbpf/ci/pull/117)
- runs the same stack then BPF CI
- can be used for local development