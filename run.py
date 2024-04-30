from mininet.node import Switch
from mininet.topo import Topo
from mininet.net import Mininet
from mininet.cli import CLI
import sys
import time
import json


RELEASE_EXECUTABLE = "./target/release/stp-rs"


class EtherSwitch(Switch):
    ''' A custom extension of the base mininet switch that
    runs the executable for each mininet switch. '''

    def __init__(self, name: str, **kwargs):
        self.name = name
        super(EtherSwitch, self).__init__(name, **kwargs)

    def start(self, controllers):
        self.cmd(
            f'mkdir -p logs && {RELEASE_EXECUTABLE} {self.name} > "logs/{self.name}-log.txt" &')

    def stop(self):
        self.cmd(f'kill {RELEASE_EXECUTABLE}')


class EtherTopo(Topo):
    def __init__(self, topo_file: str, **kwargs):
        with open(topo_file, 'r') as topo_file:
            self.topo = json.loads(topo_file.read())
        super(EtherTopo, self).__init__(**kwargs)

    def build(self):
        hosts = list(self.topo["topology"]["hosts"].keys())
        hosts.sort()
        for ind, host in enumerate(hosts):
            # mac_addr = f'02:00:00:00:00:0{ind + 1}'
            # self.addHost(host, mac=mac_addr)
            h = self.addHost(host)
            print(f"adding host {host} at {h.MAC()}")

        for switch in self.topo["topology"]["switches"]:
            print(f"adding switch: {switch}")
            self.addSwitch(switch, cls=EtherSwitch)

        for link in self.topo["topology"]["links"]:
            print(f"adding link: {link}")
            self.addLink(link[0], link[1])


def run(interactive: bool, topo_file: str):
    topo = EtherTopo(topo_file)
    net = Mininet(topo=topo)
    net.start()

    if interactive:
        CLI(net)
    else:
        sleep_sec = 1
        print(f"sleeping for {sleep_sec} sec, let STP set up")
        time.sleep(sleep_sec)
        net.pingAll()

    net.stop()


def usage():
    print("**.py [interactive/quiet -i/q] [topology filepath]")
    print("sudo python run.py -q ./topos/topo.json")


def main():
    args = sys.argv
    if len(args) != 3:
        usage()
        return

    interactive = None
    if args[1] == "-i":
        interactive = True
    elif args[1] == "-q":
        interactive = False
    else:
        usage()
        return
    run(interactive, args[2])


if __name__ == "__main__":
    main()
