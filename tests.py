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
        TEST_LEN_SEC = 10

        topo = test_runner.EtherTopo("./topo/grid.json")

        print(topo.hosts())

        net = test_runner.Mininet(topo=topo)
        net.start()

        sleep_sec = 2
        print(f"sleeping for {sleep_sec} sec, let STP set up")
        time.sleep(sleep_sec)

        ploss = net.ping(hosts=['h1', 'h5'], timeout=10)
        print(ploss)

        '''


        print(f"running iperf for {TEST_LEN_SEC} seconds")
        server_out = server.cmd(
            "mkdir -p logs && iperf -s -V -o ./logs/iperf-server.txt &")
        time.sleep(2)

        client_out = client.cmd(
            f"iperf -t {TEST_LEN_SEC} -c {server.IP()} -o ./logs/iperf-client.txt")

        print(server_out)
        print(client_out)

        print("test complete, cleaning up")
        time.sleep(2)
        server.cmd('pkill iperf')
        '''

        net.stop()
        return

    print(args[1])
    raise Exception(f"unrecognized mode, must be 'corr' or 'perf': {args[1]}")


if __name__ == "__main__":
    main()
