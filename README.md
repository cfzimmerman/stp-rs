# Self learning loop free Ethernet switches

_Cory Zimmerman_  
_CS 145, Spring 2024_

This project demonstrates functional control software for self-learning Layer 2 Ethernet switches. The Rust executable can be run on Mininet switches for full Layer 2 connectivity.

[Video](https://youtu.be/quDgQE4ZLNw?si=f-zVXcWzp9Vm02G9)

### Overview
The switch program achieves partial but not full Spanning Tree Protocol (STP) functionality. Following the protocol, switches run a state machine for each port to elect a root node by minimum MAC address and forward packets along a spanning tree.

Each port carries read and write buffers with an associated Ethernet interface. Inflow data is inspected, and host packets copied directly to the outflow buffer according to the forwarding table. Unrecognized destinations are flooded. Control packets (BPDUs) are inspected and processed according to STP rules and current port states. Switches also occasionally flood their BPDU to neighbors.

Deviations from pure STP are due largely to time and platform constraints. My BPDUs use a subset of protocol-spec packet fields, and mine use default number types for simplicity. Also, my forwarding tables are hard state, and my network does not attempt to recover from link failure. Modifying this project to handle link failure would require shifting switches toward all soft state and detecting link fail/join events. I pursued other aspects of the project instead of failure recovery because I couldn’t figure out how to simulate and test link failure in Mininet.

### Code:
Python Mininet code is used to run the networks. In [run.py](run.py), the same topology.json` format we used in the datacenter projects is parsed and built into a Mininet network. Each switch in the network runs the Rust executable.

I chose to write the switch code in Rust because that’s the language I’m most comfortable with. Mininet sandboxes the network environment, so I was able to use a normal networking library to read and write Ethernet I/O. From there, the work was just implementing an approximate version of STP.

To explore the switch code, start at [src/main.rs](src/main.rs), which consumes the library code defined in `lib.rs`. The code handling most of the switch behavior can be found at `EthSwitch::run` around line 155 of [src/stp/eth.rs](src/stp/eth.rs).

### Challenges:
The networking library I chose (`pnet`) uses channels (Rust read/write queues) to handle Ethernet IO. The API is designed for applications accessing only a single Ethernet port, and it only supports blocking reads with an optional timeout. Because my switch code needs to manage IO on arbitrarily many ethernet ports, it can’t just wait on one. On a single computer, I’d just spawn a thread for each Ethernet port and funnel tagged payloads to a  worker thread via channel, but spawning threads for each port of each switch on an emulated network on emulated Ubuntu on my laptop seemed far too heavy. Requiring a single-threaded solution also ruled out an async runtime like `Tokio`, as the `pnet` channels still ultimately require blocking reads, which would freeze a single threaded async runner as well. So, I opted to just poll each Ethernet port in a busy loop. Doing so feels wasteful and results in lots of needless context switching, but it was the only solution I found that met all my requirements. I experimented with the performance implications of different polling speeds in the experiments below.

### Testing:

I ran my network on the CS 145 VM, although I believe it should work on any Linux machine with Mininet installed.
- Install Rust: https://www.rust-lang.org/tools/install
	- Restart or source your terminal so that `cargo` is in your path.
- Clone the repo and `cd stp-rs`.
- Run `cargo build --release` to build the executable that the Mininet setup script will search for.
- Run `sudo python tests.py` to run connectivity tests. This script builds networks from all the topology files in the `topo` directory and calls `pingall` before exiting. To explore a specific topology or to run a single test, use `sudo python run.py [args]`. Call the script with no args to print options. For example, to load the triangle and start the Mininet CLI, call `sudo python run.py -i ./topo/triangle.json`. If you’re curious about the topologies, I put a picture of each next to the json file. The file `ftree16.json` is the four-port fat tree from project 1.
- Note: `pingall` has failed for me a few times, but I wasn't able to find a deterministic cause. When this happens, please stop the network, run `sudo mn -c`, and try again.

### Analysis:

This project primarily focused on correctness. The switches consistently pass the `pingall` tests for all networks, so I feel positive about their correctness in absence of link failures.

As mentioned in the challenges section, my switches poll each Ethernet port for updates in a busy loop. Each polling attempt waits for a certain amount of time before giving up and moving on. This incidentally forms a proxy for how fast the switch hardware is, so I explored the effect of different timeout values on round trip time. Additionally, because MAC addresses in my networks are randomly assigned (a random switch has the lowest MAC), every run of the network generates a new spanning tree, which also offered an opportunity to explore the impact of root placement on network performance.

I ran ping flows from the top left to the bottom right of the grid topology. Each test for each poll timeout assignment made 25 round trips, counting average RTT. Results can be found here: https://docs.google.com/spreadsheets/d/1TGr41xT13IpTGZ-qCWeuO5A9ZOJwou8CnrbcFv1s4JA/edit

A poll timeout of 100 microseconds performed best. The 2000 microsecond timeout was effectively switch hardware running 20 times slower, and the network running faster switches accomplished about five RTTs for every one RTT in the slower network. I’m guessing the 10 microsecond switches were a bit worse because my laptop was already under heavy load, and a loop this fast required more context switching than was really worthwhile.

It was also interesting seeing variance between trials. Each CSV row represents a different assignment of MAC addresses and a randomly chosen STP root. Even on a fairly balanced grid topology, I suspect unfavorable root node assignment had an impact on runs that were particularly slow compared to average. 
