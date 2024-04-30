import run as test_runner
import sys
import time


def usage():
    print("Runs correctness and performance tests on the EthSwitch network")
    print("**.py [corr/perf]")
    print("**.py corr")


def main():
    args = sys.argv
    if len(args) != 2:
        usage()
        return

    mode = None
    if args[1] == "corr":
        mode = "corr"
    elif args[1] == "perf":
        mode = "perf"
    else:
        usage()
        return

    if mode == "corr":
        test_runner.run(False, "./topo/single.json")
        test_runner.run(False, "./topo/triangle.json")
        test_runner.run(False, "./topo/grid.json")
        test_runner.run(False, "./topo/ftree16.json")
        return

    if mode == "perf":
        TEST_LEN_SEC = 60

        topo = test_runner.EtherTopo("./topo/grid.json")

        print(topo.hosts())

        net = test_runner.Mininet(topo=topo)
        net.start()

        sleep_sec = 2
        print(f"sleeping for {sleep_sec} sec, let STP set up")
        time.sleep(sleep_sec)

        server = net.get('h5')
        client = net.get('h1')

        server.cmd("mkdir -p logs && iperf -s > ./logs/iperf-server.txt &")
        client.cmd(
            f"iperf -t {TEST_LEN_SEC} -c {server.IP()} > ./logs/iperf-client.txt")

        net.stop()
        return

    print(args[1])
    raise Exception(f"unrecognized mode, must be 'corr' or 'perf': {args[1]}")


if __name__ == "__main__":
    main()
