//! Max gains of a position in terms of the quote asset.
use schemars::{
    schema::{InstanceType, SchemaObject},
    JsonSchema,
};

use crate::prelude::*;

/// String representation of positive infinity.
const POS_INF_STR: &str = "+Inf";

/// The max gains for a position.
///
/// Max gains are always specified by the user in terms of the quote currency.
///
/// Note that when opening long positions in collateral-is-base markets,
/// infinite max gains is possible. However, this is an error in the case of
/// short positions or collateral-is-quote markets.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum MaxGainsInQuote {
    /// Finite max gains
    Finite(NonZero<Decimal256>),
    /// Infinite max gains
    PosInfinity,
}

impl Display for MaxGainsInQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MaxGainsInQuote::Finite(val) => val.fmt(f),
            MaxGainsInQuote::PosInfinity => write!(f, "{}", POS_INF_STR),
        }
    }
}

impl FromStr for MaxGainsInQuote {
    type Err = PerpError;
    fn from_str(src: &str) -> Result<Self, PerpError> {
        match src {
            POS_INF_STR => Ok(MaxGainsInQuote::PosInfinity),
            _ => match src.parse() {
                Ok(number) => Ok(MaxGainsInQuote::Finite(number)),
                Err(err) => Err(perp_error!(
                    ErrorId::Conversion,
                    ErrorDomain::Default,
                    "error converting {} to MaxGainsInQuote, {}",
                    src,
                    err
                )),
            },
        }
    }
}

impl TryFrom<&str> for MaxGainsInQuote {
    type Error = anyhow::Error;

    fn try_from(val: &str) -> Result<Self, Self::Error> {
        Self::from_str(val).map_err(|err| err.into())
    }
}

impl serde::Serialize for MaxGainsInQuote {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MaxGainsInQuote::Finite(number) => number.serialize(serializer),
            MaxGainsInQuote::PosInfinity => serializer.serialize_str(POS_INF_STR),
        }
    }
}

impl<'de> serde::Deserialize<'de> for MaxGainsInQuote {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(MaxGainsInQuoteVisitor)
    }
}

impl JsonSchema for MaxGainsInQuote {
    fn schema_name() -> String {
        "MaxGainsInQuote".to_owned()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            format: Some("leverage".to_owned()),
            ..Default::default()
        }
        .into()
    }
}

struct MaxGainsInQuoteVisitor;

impl<'de> serde::de::Visitor<'de> for MaxGainsInQuoteVisitor {
    type Value = MaxGainsInQuote;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("MaxGainsInQuote")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse()
            .map_err(|_| E::custom(format!("Invalid MaxGainsInQuote: {v}")))
    }
}

impl MaxGainsInQuote {
    /// Calculate the needed counter collateral
    pub fn calculate_counter_collateral(
        self,
        market_type: MarketType,
        collateral: NonZero<Collateral>,
        notional_size_in_collateral: Signed<Collateral>,
        leverage_to_notional: SignedLeverageToNotional,
    ) -> Result<NonZero<Collateral>> {
        let direction_to_base = leverage_to_notional.direction().into_base(market_type);
        Ok(match market_type {
            MarketType::CollateralIsQuote => match self {
                MaxGainsInQuote::Finite(max_gains_in_collateral) => {
                    collateral.checked_mul_non_zero(max_gains_in_collateral)?
                }
                MaxGainsInQuote::PosInfinity => {
                    return Err(MarketError::InvalidInfiniteMaxGains {
                        market_type,
                        direction: direction_to_base,
                    }
                    .into_anyhow());
                }
            },
            MarketType::CollateralIsBase => {
                match self {
                    MaxGainsInQuote::PosInfinity => {
                        // In a Collateral-is-base market, infinite max gains are only allowed on
                        // short positions. This is because going short in this market type is betting
                        // on the asset going up (the equivalent of taking a long position in a
                        // Collateral-is-quote market). Note, the error message purposefully describes
                        // this as a "Long" position to keep things clear and consistent for the user.
                        if leverage_to_notional.direction() == DirectionToNotional::Long {
                            return Err(MarketError::InvalidInfiniteMaxGains {
                                market_type,
                                direction: direction_to_base,
                            }
                            .into_anyhow());
                        }

                        NonZero::new(notional_size_in_collateral.abs_unsigned())
                            .context("notional_size_in_collateral is zero")?
                    }
                    MaxGainsInQuote::Finite(max_gains_in_notional) => {
                        let max_gains_multiple = Number::ONE
                            - (max_gains_in_notional.into_number() + Number::ONE)
                                .checked_div(leverage_to_notional.into_number())?;

                        if max_gains_multiple.approx_lt_relaxed(Number::ZERO) {
                            return Err(MarketError::MaxGainsTooLarge {}.into());
                        }

                        let counter_collateral = collateral
                            .into_number()
                            .checked_mul(max_gains_in_notional.into_number())?
                            .checked_div(max_gains_multiple)?;
                        NonZero::<Collateral>::try_from_number(counter_collateral).with_context(|| format!("Calculated an invalid counter_collateral: {counter_collateral}"))?
                    }
                }
            }
        })
    }
}
