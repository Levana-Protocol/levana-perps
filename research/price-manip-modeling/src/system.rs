use anyhow::Result;

use crate::{
    amm::Amm,
    config::{Config, INITIAL_ASSET},
    msg::{Perps, Position},
    types::{Direction, Price, Usd, Wallet},
};

#[derive(Debug)]
pub(crate) struct System<'a> {
    blocks: u64,
    config: &'a Config,
    amm: Amm,
    pub(crate) perps: Perps,
    pub(crate) attacker_wallet: Wallet,
    arbitrage_wallet: Wallet,
    past_prices: Vec<Price>,
    attacker_positions: Vec<Position>,
}

impl<'a> System<'a> {
    pub(crate) fn new(config: &'a Config) -> Self {
        let amm = Amm::new(config);
        let perps = msg::new();
        System {
            blocks: 0,
            config,
            amm,
            perps,
            attacker_wallet: Wallet::default(),
            arbitrage_wallet: Wallet::default(),
            past_prices: vec![],
            attacker_positions: vec![],
        }
    }

    /// Simulate another block running
    pub(crate) fn step(&mut self) {
        self.blocks += 1;
        let price = self.amm.price();
        self.past_prices.push(price);

        self.arbitrage_wallet += self.amm.arbitrage(&self.config);
    }

    /// The attacker pushes the spot price up using the given amount of USD
    pub(crate) fn attack_start_spot_up(&mut self, usd: Usd) {
        self.attacker_wallet += self.amm.buy(usd);
    }

    /// The attacker closes out a spot up attack by selling asset at current price
    pub(crate) fn attacker_end_spot_up(&mut self) {
        self.attacker_wallet += self.amm.sell(self.attacker_wallet.asset);
    }

    fn twap(&self) -> Price {
        assert!(!self.past_prices.is_empty());
        assert!(self.config.twap_blocks > 0);

        let mut count = 0.0;
        let mut total = 0.0;

        self.past_prices
            .iter()
            .rev()
            .take(self.config.twap_blocks)
            .for_each(|Price(price)| {
                count += 1.0;
                total += price;
            });

        Price(total / count)
    }

    /// Close out the arbitragers, useful for viewing the final status after attacks are done
    pub(crate) fn close_arbitragers(&mut self) {
        self.arbitrage_wallet += self.amm.sell(self.arbitrage_wallet.asset);
    }

    /// Get the attacker profit and ROI
    pub(crate) fn summary(&self, attacker_capital: Usd) -> (Usd, f64) {
        (
            self.attacker_wallet.usd,
            self.attacker_wallet.usd.0 / attacker_capital.0,
        )
    }

    pub(crate) fn attack_start_perps_up(&mut self, usd: Usd, leverage: f64) -> Result<()> {
        let (wallet, position) = self.perps.open(
            Position {
                notional: usd / self.amm.price() * leverage,
                trader_collateral: usd,
                cs_collateral: usd * 30.0,
                entry_price: self.entry_price(Direction::Long),
            },
            &self.config,
        )?;
        self.attacker_wallet += wallet;
        self.attacker_positions.push(position);
        Ok(())
    }

    pub(crate) fn attack_close_perps(&mut self) {
        for position in std::mem::replace(&mut self.attacker_positions, vec![]) {
            let exit_price = self.exit_price(position.direction());
            self.attacker_wallet += self.perps.close(position, exit_price, &self.config);
        }
    }

    pub(crate) fn entry_price(&self, direction: Direction) -> Price {
        let twap = self.twap();
        if self.config.house_wins {
            let price = self.amm.price();
            match direction {
                Direction::Long => {
                    if price > twap {
                        price
                    } else {
                        twap
                    }
                }
                Direction::Short => {
                    if price < twap {
                        price
                    } else {
                        twap
                    }
                }
            }
        } else {
            twap
        }
    }

    pub(crate) fn exit_price(&self, direction: Direction) -> Price {
        let twap = self.twap();
        if self.config.house_wins {
            let price = self.amm.price();
            match direction {
                Direction::Long => {
                    if price > twap {
                        twap
                    } else {
                        price
                    }
                }
                Direction::Short => {
                    if price < twap {
                        twap
                    } else {
                        price
                    }
                }
            }
        } else {
            twap
        }
    }

    pub(crate) fn check_coherence(&self) -> Result<()> {
        let usd = self.attacker_wallet.usd
            + self.arbitrage_wallet.usd
            + self.perps.liquidity_pool
            + self.perps.trading_fees
            + self.perps.artificial_slippage;
        let asset = self.attacker_wallet.asset + self.arbitrage_wallet.asset;

        anyhow::ensure!(
            (self.amm.asset.0 - INITIAL_ASSET.0).abs() < 0.0001,
            "Asset in AMM is not delta-neutral: {}",
            self.amm.asset.0 - INITIAL_ASSET.0
        );

        anyhow::ensure!(
            asset.0.abs() < 0.0001,
            "Protocol's assets are out of balance. Attacker wallet: {}. Arbitrate wallet: {}. Unbalanced: {}",
            self.attacker_wallet.asset.0,
            self.arbitrage_wallet.asset.0,
            asset.0,
        );

        anyhow::ensure!(
            usd.0.abs() < 0.0001,
            "Protocol's USD is out of balance. Attacker: {}. Arbitrage: {}. Liquidity pool: {}. Trading fees: {}. Slippage: {}. Unbalanced: {}",
            self.attacker_wallet.usd.0,
            self.arbitrage_wallet.usd.0,
            self.perps.liquidity_pool.0,
            self.perps.trading_fees.0,
            self.perps.artificial_slippage.0,
            usd.0,
        );

        // Make sure we closed out asset positions
        anyhow::ensure!(self.attacker_wallet.asset.0 < 0.0001);
        anyhow::ensure!(self.arbitrage_wallet.asset.0 < 0.0001);

        Ok(())
    }
}
