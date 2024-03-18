use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use headless_chrome::{Browser, Element, Tab};
use scraper::{ElementRef, Html, Selector};

pub(crate) struct CoingeckoApp {
    browser: Browser,
    tab: Arc<Tab>,
    client: reqwest::blocking::Client,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ScrapePlan {
    total_exchanges: u32,
    uri: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ScrapePlan2 {
    total_exchanges: u32,
    coin_id: String,
}

#[derive(Debug)]
pub(crate) struct ExchangeInfo {
    pub(crate) name: String,
    kind: ExchangeKind,
    positive_two_depth: f64,
    negative_two_depth: f64,
    twenty_four_volume: f64,
    volume_percentage: Option<f64>,
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

// fn find_total_exchanges_scrapy<'a>(element: <'a>) -> Result<u32> {
//     let result = element.find_element("div .tw-mt-5 div [data-view-component=\"true\"]")?;
//     let result = result.get_inner_text()?;
//     find_page_info(result)
// }

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
    // Stale data sometimes shows volume percentage as hypen to
    // indicate missing data.
    let volume_percentage = process_percentage(volume).parse().ok();
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

pub(crate) fn fetch_specific_spot_page_scrape(
    mut file: File,
    skip_fetch: bool,
) -> Result<Vec<ExchangeInfo>> {
    if skip_fetch {
        return Ok(vec![]);
    } else {
        // let market_selector = "main div [data-coin-show-target=\"markets\"]";
        let selector = format!("table tbody tr");
        // selector = format!("{market_selector} table tbody tr");
        let mut buffer = String::new();
        // todo: Check if we need to even have tempfile and just pass Bytes
        file.read_to_string(&mut buffer)?;

        let document = Html::parse_document(&buffer);
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
}

pub(crate) fn get_scrape_plan_scrapy() -> Result<ScrapePlan2> {
    let market_selector = "main div [data-coin-show-target=\"markets\"]";

    let mut buffer = String::new();
    // todo: Check if we need to even have tempfile and just pass Bytes
    let mut file = std::fs::File::open("/home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/test.html")?;
    file.read_to_string(&mut buffer)?;

    let document = Html::parse_document(&buffer);
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
        let client = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .build()?;
        Ok(CoingeckoApp {
            browser,
            tab,
            client,
        })
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
            tracing::debug!("Going to find data table");
            let exchange_table = self.tab.find_elements(selector.as_str())?;
            tracing::debug!("Found exchange_table");
            for row in exchange_table {
                let item = fetch_exchange_row(row)?;
                tracing::debug!("Fetched one exchange: {}", item.name);
                exchanges.push(item);
            }
            self.tab.close(false)?;
            Ok(exchanges)
        }
    }

    pub(crate) fn download_coin_page(&self, uri: &str) -> Result<()> {
        self.tab.navigate_to(uri)?.wait_until_navigated()?;
        tracing::debug!("Gonna click market");
        self.tab.find_element("#tab-markets")?.click()?;

        let mut file = File::create("prog_test.html")?;
        let content = self.tab.get_content()?;
        file.write(content.as_bytes())?;
        Ok(())
    }

    fn download_exchange_page(&self, plan: ScrapePlan) -> Result<Vec<String>> {
        // Workaround for celing division
        let total_pages = (plan.total_exchanges + 99) / 100;
        // let mut result = vec![];
        for page in 1..=total_pages {
            tracing::debug!("Gonna scrape page {page}");
        }

        // self.tab.navigate_to(uri)?.wait_until_navigated()?;
        // let bytes = self.client.get(uri).send()?.bytes()?;
        // let mut exchange_file = tempfile::tempfile()?;
        // exchange_file.write(&bytes.to_vec())?;
        // Ok(exchange_file)
        todo!()
    }

    pub(crate) fn apply_scrape_plan(
        &self,
        plan: ScrapePlan,
        skip_fetch: bool,
    ) -> Result<Vec<ExchangeInfo>> {
        // Workaround for celing division
        let total_pages = (plan.total_exchanges + 99) / 100;
        let mut result = vec![];
        for page in 1..=total_pages {
            tracing::debug!("Gonna scrape page {page}");
            let uri = if let Some(uri) = &plan.uri {
                Some(format!(
                    "https://www.coingecko.com{}?items=10&page={page}",
                    uri
                ))
            } else {
                None
            };
            let mut exchanges = self.fetch_specific_spot_page(uri, skip_fetch)?;
            result.append(&mut exchanges);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use crate::coingecko::{
        fetch_exchange_row_scraper, fetch_specific_spot_page_scrape, find_page_info,
        my_float_conversion, process_usd_amount, CoingeckoApp, ScrapePlan,
    };

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
        let fs = File::open("/home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/spot_test_page.html").unwrap();
        let result = fetch_specific_spot_page_scrape(fs, false).unwrap();
        assert_eq!(result.len(), 100, "Fetched 100 exchanges");
    }

    // #[test]
    // fn spot_page() {
    //     let app = CoingeckoApp::new().unwrap();
    //     // todo: takes 7 minuts for 100 rows, so best to use 3 rows or something like that
    //     let result = app.fetch_specific_spot_page("file:///home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/spot_test_page.html").unwrap();
    //     assert_eq!(result.len(), 100, "Fetched 100 exchanges");
    // }

    // #[test]
    // fn scrape_plan() {
    //     let app = CoingeckoApp::new().unwrap();
    //     let result = app.get_scrape_plan("file:///home/sibi/fpco/github/levana/levana-perps/packages/perps-exes/src/bin/perps-market-params/test.html").unwrap();
    //     assert_eq!(
    //         result,
    //         ScrapePlan {
    //             total_exchanges: 309,
    //             uri: "/en/coins/1481/markets/spot".to_owned()
    //         },
    //         "Scraped plan"
    //     );
    // }

    #[test]
    fn float_plan() {
        let result = my_float_conversion(32.6).unwrap();
        assert_eq!(result, 32);

        let result = my_float_conversion(32.001).unwrap();
        assert_eq!(result, 32);

        let result = my_float_conversion(32.7).unwrap();
        assert_eq!(result, 32);
    }
}

fn my_float_conversion(input: f32) -> Result<i64> {
    Ok(input.trunc().to_string().parse()?)
}
