use anyhow::Result;
use headless_chrome::{browser, Browser};

pub(crate) struct ExchangeInfo {
    name: String,
    kind: ExchangeKind,
    pair: String,
    price: f64,
    positive_two_depth: f64,
    negative_two_depth: f64,
    twenty_four_hour_volume: f64,
    volume: f64,
    valid: bool
}

enum ExchangeKind {
    Cex,
    Dex
}

pub(crate) enum Coin {
    Atom
}

impl Coin {
    pub(crate) fn coingecko_page(&self) -> String {
        match self {
            Coin::Atom => "cosmos-hub".to_owned()
        }
    }

    pub(crate) fn coingecko_uri(&self) -> String {
        format!("https://www.coingecko.com/en/coins/{}", self.coingecko_page())
    }
}

pub(crate) fn fetch_exchange_info(coin: Coin) -> Result<Vec<ExchangeInfo>> {
    let uri = coin.coingecko_uri();
    let browser = Browser::default()?;
    println!("debug 0 {uri}");
    let tab = browser.new_tab()?;
    println!("debug 1");
    tab.navigate_to(uri.as_str())?.wait_until_navigated()?;
    println!("debug 2");
    // let png_data = tab.capture_screenshot(ScreenshotFormat::PNG, None, true)?;

    tab.find_element("#tab-markets")?.click()?;
    println!("debug 3");
    let table = tab.find_element("tbody.tw-divide-y")?;
    println!("debug: {table:?}");
    // todo: click 100
    // todo: find total pages and do pagination
    // func get_data() {
    //
    //}
    Ok(vec![])
}
