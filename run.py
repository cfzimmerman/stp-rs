from mininet.node import Switch
from mininet.topo import Topo
from mininet.net import Mininet
from mininet.cli import CLI


RELEASE_EXECUTABLE = "./target/release/stp-rs"


class EtherSwitch(Switch):
    ''' A custom extension of the base mininet switch that
    runs the executable for each mininet switch. '''

    def __init__(self, name, **kwargs):
        self.name = name
        super(EtherSwitch, self).__init__(name, **kwargs)

    def start(self, controllers):
        self.cmd(
            f'{RELEASE_EXECUTABLE} {self.name} > "logs/{self.name}-log.txt" &')

    def stop(self):
        self.cmd(f'kill {RELEASE_EXECUTABLE}')


class EtherTopo(Topo):
    def build(self):
        # TODO: make the topology parameter driven

        s1 = self.addSwitch('s1', cls=EtherSwitch, mac=f'00:00:00:00:00:0{1}')
        s2 = self.addSwitch('s2', cls=EtherSwitch)
        s3 = self.addSwitch('s3', cls=EtherSwitch)

        h1 = self.addHost('h1')
        h2 = self.addHost('h2')
        h3 = self.addHost('h3')

        self.addLink(h1, s1)
        self.addLink(h2, s2)
        self.addLink(h3, s3)

        self.addLink(s1, s2)
        self.addLink(s1, s3)
        self.addLink(s2, s3)

        # s1 = self.addSwitch('s1', cls=EtherSwitch)

        # h1 = self.addHost('h1')
        # h2 = self.addHost('h2')

        # self.addLink(h1, s1)
        # self.addLink(h2, s1)


def run():
    topo = EtherTopo()
    net = Mininet(topo=topo)
    net.start()

    # net.pingAll()
    CLI(net)

    net.stop()


if __name__ == "__main__":
    run()
