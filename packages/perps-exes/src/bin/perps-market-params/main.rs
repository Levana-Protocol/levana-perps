use coingecko::Coin;

use crate::coingecko::fetch_exchange_info;

mod coingecko;

fn main() {
    fetch_exchange_info(Coin::Atom).unwrap();
}
