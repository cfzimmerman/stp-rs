use pnet::datalink;

struct EthRouter {}

impl EthRouter {
    pub fn build() -> anyhow::Result<Self> {
        for intf in datalink::interfaces() {
            println!("{:#?}", intf);
        }
        Ok(EthRouter {})
    }
}

fn main() -> anyhow::Result<()> {
    EthRouter::build()?;
    Ok(())
}
