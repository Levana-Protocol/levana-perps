use crate::{
    config::Config,
    types::{AmmK, Asset, Price, Usd, Wallet},
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Amm {
    pub(crate) usd: Usd,
    pub(crate) asset: Asset,
}

impl Amm {
    pub(crate) fn new(config: &Config) -> Self {
        Amm {
            usd: Usd(config.amm_volume.0 * config.expected_spot.0),
            asset: config.amm_volume,
        }
    }

    pub(crate) fn price(&self) -> Price {
        self.usd / self.asset
    }

    /// Perform a single arbitrage step
    ///
    /// Returns the delta to the arbitragers wallet.
    pub(crate) fn arbitrage(&mut self, config: &Config) -> Wallet {
        let price = self.price();

        let epsilon = ((price.0 - config.expected_spot.0) / config.expected_spot.0).abs();

        if epsilon < config.arbitrage_epsilon {
            Wallet::default()
        } else {
            // Simulate the action and only take it if we don't blow past the expected
            let mut simulate = *self;
            let (should_use, wallet) = if price > config.expected_spot {
                // Time to sell some OSMO. Convert USDC to OSMO to be added to the pool.
                let asset = config.arbitrage_rate / price;
                let wallet = simulate.sell(asset);
                (simulate.price() > config.expected_spot, wallet)
            } else {
                let wallet = simulate.buy(config.arbitrage_rate);
                (simulate.price() < config.expected_spot, wallet)
            };

            if should_use {
                *self = simulate;
                wallet
            } else {
                log::debug!("Simulated an arbitrage but it overcorrected the price, skipping");
                Wallet::default()
            }
        }
    }

    fn get_k(&self) -> AmmK {
        self.usd * self.asset
    }

    pub(crate) fn sell(&mut self, asset: Asset) -> Wallet {
        let k = self.get_k();
        self.asset += asset;
        let new_usd = k / self.asset;
        let old_usdc = self.usd;
        self.usd = new_usd;
        Wallet {
            usd: Usd(old_usdc.0 - new_usd.0),
            asset: Asset(-asset.0),
        }
    }

    pub(crate) fn buy(&mut self, usd: Usd) -> Wallet {
        let k = self.get_k();
        self.usd.0 += usd.0;
        let new_asset = k / self.usd;
        let old_asset = self.asset;
        self.asset = new_asset;
        Wallet {
            asset: Asset(old_asset.0 - new_asset.0),
            usd: Usd(-usd.0),
        }
    }
}
