use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use headless_chrome::{Browser, Tab};
use scraper::{ElementRef, Html, Selector};

pub(crate) struct CoingeckoApp {
    #[allow(dead_code)]
    browser: Browser,
    tab: Arc<Tab>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ScrapePlan {
    total_exchanges: u32,
    uri: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct ScrapePlan2 {
    total_exchanges: u32,
    coin_id: String,
}

#[derive(Debug)]
pub(crate) struct ExchangeInfo {
    pub(crate) name: String,
    pub(crate) kind: ExchangeKind,
    pub(crate) positive_two_depth: f64,
    pub(crate) negative_two_depth: f64,
    pub(crate) twenty_four_volume: f64,
    pub(crate) volume_percentage: Option<f64>,
    pub(crate) stale: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ExchangeKind {
    Cex,
    Dex,
}

impl TryFrom<String> for ExchangeKind {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::prelude::v1::Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "cex" => Ok(ExchangeKind::Cex),
            "dex" => Ok(ExchangeKind::Dex),
            other => Err(anyhow!("Invalid exchange type: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Coin {
    Atom,
    Levana,
}

impl From<Coin> for String {
    fn from(value: Coin) -> Self {
        match value {
            Coin::Atom => "cosmos-hub".to_owned(),
            Coin::Levana => "levana".to_owned(),
        }
    }
}

impl TryFrom<String> for Coin {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::prelude::v1::Result<Self, Self::Error> {
        match &value[..] {
            "cosmos-hub" => Ok(Coin::Atom),
            "levana" => Ok(Coin::Levana),
            other => Err(anyhow!("Unrecognized coin {other}")),
        }
    }
}

impl Coin {
    pub(crate) fn coingecko_uri(self) -> String {
        let coin_id = Into::<String>::into(self);
        format!("https://www.coingecko.com/en/coins/{coin_id}")
    }
}

fn find_page_info(info: String) -> Result<u32> {
    let items = info.split_whitespace();
    let items = items.skip(4);
    let total = items.skip(1).next().context("Missing total element")?;
    let total: u32 = total.parse()?;
    Ok(total)
}

fn process_usd_amount(amount: String) -> String {
    amount.chars().filter(|c| !['$', ','].contains(c)).collect()
}

fn process_percentage(amount: String) -> String {
    amount.chars().filter(|c| !['%'].contains(c)).collect()
}

fn fetch_exchange_row_scraper<'a>(node: ElementRef<'a>) -> Result<ExchangeInfo> {
    let initial_selector =
        Selector::parse("td").map_err(|_| anyhow!("Error constructing td selector"))?;

    let columns = node.select(&initial_selector);

    let mut columns = columns.skip(1);
    let exchange_name = columns.next().context("Missing column for exchange_name")?;
    let name = exchange_name.inner_html().trim().to_owned();
    let exchange_type = columns.next().context("Missing column for exchange_type")?;
    let span_div =
        Selector::parse("span div").map_err(|_| anyhow!("Error constructing span div selector"))?;
    let exchange_type = exchange_type
        .select(&span_div)
        .next()
        .context("Missing span div in exchange_type")?
        .inner_html()
        .trim()
        .to_owned();
    let kind: ExchangeKind = exchange_type.try_into()?;
    let mut columns = columns.skip(3);
    let positive_two_depth = columns.next().context("Missing column for +2% depth")?;
    let positive_two_depth = positive_two_depth
        .attr("data-sort")
        .context("No data-sourt attribute found for +2% depth")?
        .parse()?;
    let negative_two_depth = columns.next().context("Missing column for -2% depth")?;
    let negative_two_depth = negative_two_depth
        .attr("data-sort")
        .context("No data-sourt attribute found for -2% depth")?
        .parse()?;
    let twenty_four_volume = columns.next().context("Missing column for 24h volume")?;
    let span = Selector::parse("span").map_err(|_| anyhow!("Error constructing div selector"))?;
    let twenty_four_volume = twenty_four_volume
        .select(&span)
        .next()
        .context("Missing span in twenty_four_volume")?
        .inner_html()
        .trim()
        .to_owned();
    let twenty_four_volume = process_usd_amount(twenty_four_volume).parse()?;
    let volume = columns.next().context("Missing column for volume%")?;
    let volume = volume.inner_html().trim().to_owned();
    // Stale data sometimes shows volume percentage as hypen to
    // indicate missing data.
    let volume_percentage = process_percentage(volume).parse().ok();
    let last_updated = columns.next().context("Missing column for Last updated")?;
    let div_span =
        Selector::parse("div span").map_err(|_| anyhow!("Error constructing div span selector"))?;
    let last_updated = last_updated
        .select(&div_span)
        .next()
        .context("Missing div span in last_updated")?
        .inner_html()
        .trim()
        .to_owned();
    let stale = !last_updated.to_lowercase().starts_with("recent");
    Ok(ExchangeInfo {
        name,
        kind,
        positive_two_depth,
        negative_two_depth,
        twenty_four_volume,
        volume_percentage,
        stale,
    })
}

pub(crate) fn fetch_specific_spot_page_scrape(exchange_page: &str) -> Result<Vec<ExchangeInfo>> {
    let selector = format!("table tbody tr");

    let document = Html::parse_document(exchange_page);
    let table_selector =
        Selector::parse(&selector).map_err(|_| anyhow!("Error constructing table selector"))?;

    let mut exchanges = vec![];
    tracing::debug!("Going to find data table");
    let exchange_table = document.select(&table_selector);
    tracing::debug!("Found exchange_table");
    for row in exchange_table {
        let item = fetch_exchange_row_scraper(row)?;
        tracing::debug!("Fetched one exchange: {}", item.name);
        exchanges.push(item);
    }
    Ok(exchanges)
}

pub(crate) fn get_scrape_plan_scrapy(coin_page: &str) -> Result<ScrapePlan2> {
    let market_selector = "main div [data-coin-show-target=\"markets\"]";

    let document = Html::parse_document(&coin_page);
    let market_s = Selector::parse(&market_selector)
        .map_err(|_| anyhow!("Error constructing market_selector"))?;

    let element = document
        .select(&market_s)
        .next()
        .context("No element found for market_selector")?;

    let plan_selector = Selector::parse("div .tw-mt-5 div [data-view-component=\"true\"]")
        .map_err(|_| anyhow!("Error constructing market_selector"))?;

    let element = element
        .select(&plan_selector)
        .next()
        .context("No element found for plan_selector")?;

    let total_records = element.inner_html().trim().to_owned();
    let total_exchanges = find_page_info(total_records)?;

    let coin_id = r#"span[data-converter-target="price"]"#;
    let coin_id_selector =
        Selector::parse(coin_id).map_err(|_| anyhow!("Error constructing coin_id"))?;
    let coin_id = document
        .select(&coin_id_selector)
        .next()
        .context("Missing coin_id")?
        .attr("data-coin-id")
        .context("No attribute named data-coin-id")?
        .to_owned();

    Ok(ScrapePlan2 {
        total_exchanges,
        coin_id,
    })
}

impl CoingeckoApp {
    pub(crate) fn new() -> Result<Self> {
        let browser = Browser::default()?;
        let tab = browser.new_tab()?;
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
        tab.set_user_agent(user_agent, None, None)?;
        Ok(CoingeckoApp { browser, tab })
    }

    pub(crate) fn download_coin_page(&self, uri: &str) -> Result<String> {
        self.tab.navigate_to(uri)?.wait_until_navigated()?;
        tracing::debug!("Gonna click market");
        self.tab.find_element("#tab-markets")?.click()?;

        let content = self.tab.get_content()?;
        Ok(content)
    }

    pub(crate) fn download_exchange_pages(&self, plan: &ScrapePlan2) -> Result<Vec<String>> {
        // Workaround for celing division
        let mut total_pages = (plan.total_exchanges + 99) / 100;
        total_pages += 1;
        let mut results = vec![];
        for page in 1..total_pages {
            tracing::debug!("Gonna download exchange page {page}");
            let uri = format!(
                "https://www.coingecko.com/en/coins/{}/markets/spot?items=100&page={page}",
                plan.coin_id
            );
            self.tab.navigate_to(&uri)?.wait_until_navigated()?;
            let content = self.tab.get_content()?;
            results.push(content)
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read, path::PathBuf};

    use crate::coingecko::{
        fetch_specific_spot_page_scrape, find_page_info, get_scrape_plan_scrapy,
        process_usd_amount, ScrapePlan2,
    };

    #[test]
    fn total_info_parsing() {
        let result = find_page_info("Showing 1 to 10 of 309 results".to_owned()).unwrap();
        assert_eq!(result, 309, "Total computed pages");
    }

    #[test]
    fn usd_amount() {
        let result = process_usd_amount("35,000$".to_owned());
        assert_eq!(result, "35000", "Usd amount properly processed");
    }

    #[test]
    fn spot_page() {
        let mut spot_page = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        spot_page.push("src/bin/perps-market-params/assets/spot_test_page.html");
        let mut fs = File::open(spot_page).unwrap();
        let mut exchange_page = String::new();
        fs.read_to_string(&mut exchange_page).unwrap();
        let result = fetch_specific_spot_page_scrape(&exchange_page).unwrap();
        assert_eq!(result.len(), 100, "Fetched 100 exchanges");
    }

    #[test]
    fn scrape_plan() {
        let mut coin_page = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        coin_page.push("src/bin/perps-market-params/assets/coin_page.html");
        let mut fs = File::open(coin_page).unwrap();
        let mut coin_page = String::new();
        fs.read_to_string(&mut coin_page).unwrap();
        let result = get_scrape_plan_scrapy(&coin_page).unwrap();
        assert_eq!(
            result,
            ScrapePlan2 {
                total_exchanges: 309,
                coin_id: "1481".to_owned()
            },
            "Scraped plan"
        );
    }
}

pub(crate) fn get_exchanges(app: CoingeckoApp, coin: Coin) -> Result<Vec<ExchangeInfo>> {
    let coin_uri = coin.coingecko_uri();
    let coin_page = app.download_coin_page(&coin_uri)?;

    let plan = get_scrape_plan_scrapy(&coin_page)?;
    tracing::debug!("Computed plan: {plan:?}");

    let exchanges = app.download_exchange_pages(&plan)?;
    let mut result = vec![];
    for exchange in exchanges {
        tracing::debug!("Going fetch from exchange");
        let mut coin_exchanges = fetch_specific_spot_page_scrape(&exchange)?;
        result.append(&mut coin_exchanges);
    }
    Ok(result)
}
