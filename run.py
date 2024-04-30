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
            f'{RELEASE_EXECUTABLE} {self.name} > "logs/{self.name}-log.txt" &')

    def stop(self):
        self.cmd(f'kill {RELEASE_EXECUTABLE}')


class EtherTopo(Topo):
    def __init__(self, topo_file: str, **kwargs):
        super(EtherTopo, self).__init__(**kwargs)
        self.topo_file = topo_file

    def build(self):
        with open(self.topo_file, 'r') as topo_file:
            topo = json.loads(topo_file.read())
            print(topo)
            hosts = list(topo["topology"]["hosts"].keys())
            print(hosts)
            hosts.sort()
            for ind, host in enumerate(hosts):
                mac_addr = f'02:00:00:00:00:0{ind + 1}'
                print(f"adding host {host} at {mac_addr}")
                self.addHost(host, mac=mac_addr)

            for switch in topo["topology"]["switches"]:
                print(f"adding switch: {switch}")
                self.addSwitch(switch, cls=EtherSwitch)

            for link in topo["topology"]["links"]:
                print(f"adding link: {link}")
                self.addLink(link[0], link[1])

        '''
        s1 = self.addSwitch('s1', cls=EtherSwitch)
        s2 = self.addSwitch('s2', cls=EtherSwitch)
        s3 = self.addSwitch('s3', cls=EtherSwitch)

        h1 = self.addHost('h1', mac=f'02:00:00:00:00:0{1}')
        h2 = self.addHost('h2', mac=f'02:00:00:00:00:0{2}')
        h3 = self.addHost('h3', mac=f'02:00:00:00:00:0{3}')

        self.addLink(h1, s1)
        self.addLink(h2, s2)
        self.addLink(h3, s3)

        self.addLink(s1, s2)
        self.addLink(s1, s3)
        self.addLink(s2, s3)
        '''

        # s1 = self.addSwitch('s1', cls=EtherSwitch)

        # h1 = self.addHost('h1')
        # h2 = self.addHost('h2')

        # self.addLink(h1, s1)
        # self.addLink(h2, s1)


def run(interactive: bool, topo_file: str):
    topo = EtherTopo(topo_file)
    net = Mininet(topo=topo)
    net.start()

    if interactive:
        CLI(net)
    else:
        # Give the network time to run stp
        time.sleep(1)
        net.pingAll()

    net.stop()


def usage():
    print("**.py [bool interactive] [topology filepath]")
    print("sudo python run.py true ./topos/topo.json")


if __name__ == "__main__":
    args = sys.argv
    if len(args) != 3:
        usage()
    else:
        run(bool(args[1]), args[2])
