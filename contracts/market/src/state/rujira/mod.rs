pub(crate) mod enshrined_oracle;
pub(crate) mod grpc;

pub struct ChainSymbol<'a>(&'a str);

impl<'a> ChainSymbol<'a> {
    pub fn parse(input: &'a str) -> Self {
        let symbol = input.split_once('.').map(|(_, b)| b).unwrap_or(input);
        ChainSymbol(symbol)
    }

    pub fn symbol(&self) -> &'a str {
        self.0
    }
}
