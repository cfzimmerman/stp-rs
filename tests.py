import run as test_runner

if __name__ == "__main__":
    test_runner.run(False, "./topo/single.json")
    test_runner.run(False, "./topo/triangle.json")
    test_runner.run(False, "./topo/grid.json")
    test_runner.run(False, "./topo/ftree16.json")
