use std::time::Duration;
use stp_rs::stp::eth::EthRouter;

fn main() -> anyhow::Result<()> {
    let switch = EthRouter::build(Duration::from_secs(10), Some(Duration::from_micros(1000)))?;
    Ok(switch.run()?)
}
