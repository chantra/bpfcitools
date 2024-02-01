# Local development for BPF

This post aims to show one (IMO simple) way to do local development for BPF.

It re-uses [`danobi/vmtest`](https://github.com/danobi/vmtest) for quickly testing the changes and kind of build upon what is presented in [Running BPF tests in BPF CI environment](bpfci-troubleshooting.md).

## Building a VM friendly kernel for BPF

This is assuming you already have a checkout of [`bpf-next`](https://git.kernel.org/pub/scm/linux/kernel/git/bpf/bpf-next.git/) or such, have already have all the toolchain on your host.
```
# Set up our kconfig
cat tools/testing/selftests/bpf/config{,.$(uname -m),.vm} > .config
make olddefconfig
# Build the kernel
make -j$((4* $(nproc)))
# Build BPF selftests
make -j$((4* $(nproc))) -C tools/testing/selftests/bpf 
```

## Running tests

From there we can run test with:

```
$ vmtest -k $(make -s image_name) "cd tools/testing/selftests/bpf/ && ./test_progs -t lwt_redirect/lwt_redirect_normal"
...
...

=> bzImage
===> Booting
===> Setting up VM
===> Running command
[    2.296924] bpf_testmod: loading out-of-tree module taints kernel.
[    2.297507] bpf_testmod: module verification failed: signature and/or required key missing - tainting kernel
net.ipv6.conf.all.disable_ipv6 = 1
net.ipv6.conf.all.disable_ipv6 = 1
#142/1   lwt_redirect/lwt_redirect_normal:OK
#142/2   lwt_redirect/lwt_redirect_normal_nomac:OK
#142     lwt_redirect:OK
Summary: 1/2 PASSED, 0 SKIPPED, 0 FAILED
```

The kernel takes a couple of seconds to boot up, so iterating is pretty fast regarless of running the test in a VM or not.

## Hacking example:

To illustrate, let's reproduce the issue from https://lore.kernel.org/all/20240131053212.2247527-1-chantr4@gmail.com/ 

by applying:

```diff
diff --git a/tools/testing/selftests/bpf/prog_tests/lwt_redirect.c b/tools/testing/selftests/bpf/prog_tests/lwt_redirect.c
index b5b9e74b1044..f27e411baeff 100644
--- a/tools/testing/selftests/bpf/prog_tests/lwt_redirect.c
+++ b/tools/testing/selftests/bpf/prog_tests/lwt_redirect.c
@@ -145,7 +145,9 @@ static int expect_icmp(char *buf, ssize_t len)
        if (len < (ssize_t)sizeof(*eth))
                return -1;

-       if (eth->h_proto == htons(ETH_P_IP))
+       int proto = ntohs(eth->h_proto);
+       printf("Received packet for protocol 0x%04X with length %ld\n", proto, len);
+       if (proto == ETH_P_IP)
                return __expect_icmp_ipv4((char *)(eth + 1), len - sizeof(*eth));

        return -1;
@@ -168,6 +170,7 @@ static void send_and_capture_test_packets(const char *test_name, int tap_fd,

        filter_t filter = need_mac ? expect_icmp : expect_icmp_nomac;

+       sleep(5);
        ping_dev(target_dev, false);

        ret = wait_for_packet(tap_fd, filter, &timeo);
@@ -203,7 +206,6 @@ static int setup_redirect_target(const char *target_dev, bool need_mac)
        if (!ASSERT_GE(target_index, 0, "if_nametoindex"))
                goto fail;

-       SYS(fail, "sysctl -w net.ipv6.conf.all.disable_ipv6=1");
        SYS(fail, "ip link add link_err type dummy");
        SYS(fail, "ip link set lo up");
        SYS(fail, "ip addr add dev lo " LOCAL_SRC "/32");
```

We re-enable IPv6, sleep 5 second to give more time for IPv6 traffic to make it to tap0 and also print
the protocol and size of the traffic received on tap0.

We rebuild `test_progs`:
```
make -j$((4* $(nproc))) -C tools/testing/selftests/bpf test_progs
```

And re-run the VM:
```
$ vmtest -k $(make -s image_name) "cd tools/testing/selftests/bpf/ && ./test_progs -t lwt_redirect/lwt_redirect_normal"
=> bzImage
===> Booting
===> Setting up VM
===> Running command
[    2.345878] bpf_testmod: loading out-of-tree module taints kernel.
[    2.346604] bpf_testmod: module verification failed: signature and/or required key missing - tainting kernel
test_lwt_redirect:PASS:pthread_create 0 nsec
test_lwt_redirect:PASS:pthread_join 0 nsec
test_lwt_redirect_run:PASS:netns_create 0 nsec
open_netns:PASS:malloc token 0 nsec
open_netns:PASS:open /proc/self/ns/net 0 nsec
open_netns:PASS:open netns fd 0 nsec
open_netns:PASS:setns 0 nsec
test_lwt_redirect_run:PASS:setns 0 nsec
open_tuntap:PASS:open(/dev/net/tun) 0 nsec
open_tuntap:PASS:ioctl(TUNSETIFF) 0 nsec
open_tuntap:PASS:fcntl(O_NONBLOCK) 0 nsec
setup_redirect_target:PASS:open_tuntap 0 nsec
setup_redirect_target:PASS:if_nametoindex 0 nsec
setup_redirect_target:PASS:ip link add link_err type dummy 0 nsec
setup_redirect_target:PASS:ip link set lo up 0 nsec
setup_redirect_target:PASS:ip addr add dev lo 10.0.0.1/32 0 nsec
setup_redirect_target:PASS:ip link set link_err up 0 nsec
setup_redirect_target:PASS:ip link set tap0 up 0 nsec
setup_redirect_target:PASS:ip route add 10.0.0.0/24 dev link_err encap bpf xmit obj test_lwt_redirect.bpf.o sec redir_ingress 0 nsec
setup_redirect_target:PASS:ip route add 20.0.0.0/24 dev link_err encap bpf xmit obj test_lwt_redirect.bpf.o sec redir_egress 0 nsec
test_lwt_redirect_normal:PASS:setup_redirect_target 0 nsec
ping_dev:PASS:if_nametoindex 0 nsec
Received packet for protocol 0x86DD with length 90
Received packet for protocol 0x86DD with length 86
Received packet for protocol 0x86DD with length 90
Received packet for protocol 0x86DD with length 90
Received packet for protocol 0x86DD with length 70
send_and_capture_test_packets:FAIL:wait_for_epacket unexpected wait_for_epacket: actual 0 != expected 1
(/home/chantra/devel/bpf-next/tools/testing/selftests/bpf/prog_tests/lwt_redirect.c:178: errno: Success) test_lwt_redirect_normal egress test fails
close_netns:PASS:setns 0 nsec
#142/1   lwt_redirect/lwt_redirect_normal:FAIL
#142/2   lwt_redirect/lwt_redirect_normal_nomac:OK
#142     lwt_redirect:FAIL
```

> [!NOTE]
> Because we only modify the tests, and not the kernel, we could technically get an interactive shell and re-run the tests from the host-share, unfortunately, qemu is not revalidating the host files when they are modified: https://lore.kernel.org/lkml/CAFkjPTmVbyuA0jEAjYhsOsg-SE99yXgehmjqUZb4_uWS_L-ZTQ@mail.gmail.com/