use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use headless_chrome::{Browser, Element, Tab};

pub(crate) struct CoingeckoApp {
    browser: Browser,
    tab: Arc<Tab>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ScrapePlan {
    total_exchanges: u32,
    uri: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ExchangeInfo {
    pub(crate) name: String,
    kind: ExchangeKind,
    positive_two_depth: f64,
    negative_two_depth: f64,
    twenty_four_volume: f64,
    volume_percentage: f64,
    stale: bool,
}

#[derive(Debug)]
enum ExchangeKind {
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

fn find_total_exchanges<'a>(element: Element<'a>) -> Result<u32> {
    let result = element.find_element("div .tw-mt-5 div [data-view-component=\"true\"]")?;
    let result = result.get_inner_text()?;
    find_page_info(result)
}

fn process_usd_amount(amount: String) -> String {
    amount.chars().filter(|c| !['$', ','].contains(c)).collect()
}

fn process_percentage(amount: String) -> String {
    amount.chars().filter(|c| !['%'].contains(c)).collect()
}

fn fetch_exchange_row<'a>(element: Element<'a>) -> Result<ExchangeInfo> {
    let columns = element.find_elements("td")?;
    let columns = columns.iter();
    let mut columns = columns.skip(1);
    let exchange_name = columns.next().context("Missing column for exchange_name")?;
    let name = exchange_name.get_inner_text()?;
    let exchange_type = columns.next().context("Missing column for exchange_type")?;
    let exchange_type = exchange_type.find_element("span div")?.get_inner_text()?;
    let kind: ExchangeKind = exchange_type.try_into()?;
    let mut columns = columns.skip(3);
    let positive_two_depth = columns.next().context("Missing column for +2% depth")?;
    let positive_two_depth = positive_two_depth
        .get_attribute_value("data-sort")?
        .context("No data-sourt attribute found for +2% depth")?
        .parse()?;
    let negative_two_depth = columns.next().context("Missing column for -2% depth")?;
    let negative_two_depth = negative_two_depth
        .get_attribute_value("data-sort")?
        .context("No data-sourt attribute found for -2% depth")?
        .parse()?;
    let twenty_four_volume = columns.next().context("Missing column for 24h volume")?;
    let twenty_four_volume = twenty_four_volume.find_element("span")?;
    let twenty_four_volume = twenty_four_volume.get_inner_text()?;
    let twenty_four_volume = process_usd_amount(twenty_four_volume).parse()?;
    let volume = columns.next().context("Missing column for volume%")?;
    let volume = volume.get_inner_text()?;
    let volume_percentage = process_percentage(volume).parse()?;
    let last_updated = columns.next().context("Missing column for Last updated")?;
    let last_updated = last_updated.find_element("div span")?.get_inner_text()?;
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

impl CoingeckoApp {
    pub(crate) fn new() -> Result<Self> {
        let browser = Browser::default()?;
        let tab = browser.new_tab()?;
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
        tab.set_user_agent(user_agent, None, None)?;
        Ok(CoingeckoApp { browser, tab })
    }

    pub(crate) fn fetch_specific_spot_page(
        &self,
        uri: Option<String>,
        skip_fetch: bool,
    ) -> Result<Vec<ExchangeInfo>> {
        let market_selector = "main div [data-coin-show-target=\"markets\"]";
        let mut selector = format!("table tbody tr");
        if let Some(uri) = uri {
            self.tab.navigate_to(uri.as_str())?.wait_until_navigated()?;
        } else {
            selector = format!("{market_selector} table tbody tr");
        }
        if skip_fetch {
            return Ok(vec![]);
        } else {
            let mut exchanges = vec![];
            let exchange_table = self.tab.find_elements(selector.as_str())?;
            for row in exchange_table {
                let item = fetch_exchange_row(row)?;
                tracing::debug!("Fetched one exchange: {}", item.name);
                exchanges.push(item);
            }
            Ok(exchanges)
        }
    }

    pub(crate) fn get_scrape_plan(&self, uri: &str) -> Result<ScrapePlan> {
        self.tab.navigate_to(uri)?.wait_until_navigated()?;
        tracing::debug!("Gonna click markets");
        self.tab.find_element("#tab-markets")?.click()?;

        let market_selector = "main div [data-coin-show-target=\"markets\"]";
        let element = self.tab.find_element(market_selector)?;
        let total_exchanges = find_total_exchanges(element)?;
        tracing::debug!("Found total pages: {total_exchanges}");
        let uri = if total_exchanges > 9 {
            let next_selector = format!("{market_selector} nav span [rel=\"next\"]");
            let next_element = self.tab.find_element(next_selector.as_str())?;
            let next_uri = next_element
                .get_attribute_value("data-url")?
                .context("Missing data-url attribute")?;
            let uri = next_uri
                .split_terminator('?')
                .next()
                .context("No ? found in uri")?
                .to_owned();
            Some(uri)
        } else {
            None
        };

        Ok(ScrapePlan {
            total_exchanges,
            uri,
        })
    }

    pub(crate) fn apply_scrape_plan(&self, plan: ScrapePlan) -> Result<Vec<ExchangeInfo>> {
        // Workaround for celing division
        let total_pages = (plan.total_exchanges + 99) / 100;
        let mut result = vec![];
        for page in 1..=total_pages {
            tracing::debug!("Gonna scrape page {page}");
            let uri = if let Some(uri) = &plan.uri {
                Some(format!(
                    "https://www.coingecko.com{}?items=100&page={page}",
                    uri
                ))
            } else {
                None
            };
            let mut exchanges = self.fetch_specific_spot_page(uri, true)?;
            result.append(&mut exchanges);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::coingecko::{find_page_info, process_usd_amount, CoingeckoApp, ScrapePlan};

    #[test]
    fn total_info_parsing() {
        let result = find_page_info("Showing 1 to 10 of 309 results".to_owned()).unwrap();
        assert_eq!(result, 31, "Total computed pages");
    }

    #[test]
    fn usd_amount() {
        let result = process_usd_amount("35,000$".to_owned());
        assert_eq!(result, "35000", "Usd amount properly processed");
    }

    #[test]
    fn spot_page() {
        let app = CoingeckoApp::new().unwrap();
        // todo: takes 7 minuts for 100 rows, so best to use 3 rows or something like that
        let result = app.fetch_specific_spot_page("file:///home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/spot_test_page.html").unwrap();
        assert_eq!(result.len(), 100, "Fetched 100 exchanges");
    }

    #[test]
    fn scrape_plan() {
        let app = CoingeckoApp::new().unwrap();
        let result = app.get_scrape_plan("file:///home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/test.html").unwrap();
        assert_eq!(
            result,
            ScrapePlan {
                total_exchanges: 309,
                uri: "/en/coins/1481/markets/spot".to_owned()
            },
            "Scraped plan"
        );
    }
}
