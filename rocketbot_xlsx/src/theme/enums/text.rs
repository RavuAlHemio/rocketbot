#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextAutoNumberScheme {
    AlphaLcParenBoth,
    AlphaUcParenBoth,
    AlphaLcParenR,
    AlphaUcParenR,
    AlphaLcPeriod,
    AlphaUcPeriod,
    ArabicParenBoth,
    ArabicParenR,
    ArabicPeriod,
    ArabicPlain,
    RomanLcParenBoth,
    RomanUcParenBoth,
    RomanLcParenR,
    RomanUcParenR,
    RomanLcPeriod,
    RomanUcPeriod,
    CircleNumDbPlain,
    CircleNumWdBlackPlain,
    CircleNumWdWhitePlain,
    ArabicDbPeriod,
    ArabicDbPlain,
    Ea1ChsPeriod,
    Ea1ChsPlain,
    Ea1ChtPeriod,
    Ea1ChtPlain,
    Ea1JpnChsDbPeriod,
    Ea1JpnKorPlain,
    Ea1JpnKorPeriod,
    Arabic1Minus,
    Arabic2Minus,
    Hebrew2Minus,
    ThaiAlphaPeriod,
    ThaiAlphaParenR,
    ThaiAlphaParenBoth,
    ThaiNumPeriod,
    ThaiNumParenR,
    ThaiNumParenBoth,
    HindiAlphaPeriod,
    HindiNumPeriod,
    HindiNumParenR,
    HindiAlpha1Period,
}
impl TextAutoNumberScheme {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "alphaLcParenBoth" => Some(Self::AlphaLcParenBoth),
            "alphaUcParenBoth" => Some(Self::AlphaUcParenBoth),
            "alphaLcParenR" => Some(Self::AlphaLcParenR),
            "alphaUcParenR" => Some(Self::AlphaUcParenR),
            "alphaLcPeriod" => Some(Self::AlphaLcPeriod),
            "alphaUcPeriod" => Some(Self::AlphaUcPeriod),
            "arabicParenBoth" => Some(Self::ArabicParenBoth),
            "arabicParenR" => Some(Self::ArabicParenR),
            "arabicPeriod" => Some(Self::ArabicPeriod),
            "arabicPlain" => Some(Self::ArabicPlain),
            "romanLcParenBoth" => Some(Self::RomanLcParenBoth),
            "romanUcParenBoth" => Some(Self::RomanUcParenBoth),
            "romanLcParenR" => Some(Self::RomanLcParenR),
            "romanUcParenR" => Some(Self::RomanUcParenR),
            "romanLcPeriod" => Some(Self::RomanLcPeriod),
            "romanUcPeriod" => Some(Self::RomanUcPeriod),
            "circleNumDbPlain" => Some(Self::CircleNumDbPlain),
            "circleNumWdBlackPlain" => Some(Self::CircleNumWdBlackPlain),
            "circleNumWdWhitePlain" => Some(Self::CircleNumWdWhitePlain),
            "arabicDbPeriod" => Some(Self::ArabicDbPeriod),
            "arabicDbPlain" => Some(Self::ArabicDbPlain),
            "ea1ChsPeriod" => Some(Self::Ea1ChsPeriod),
            "ea1ChsPlain" => Some(Self::Ea1ChsPlain),
            "ea1ChtPeriod" => Some(Self::Ea1ChtPeriod),
            "ea1ChtPlain" => Some(Self::Ea1ChtPlain),
            "ea1JpnChsDbPeriod" => Some(Self::Ea1JpnChsDbPeriod),
            "ea1JpnKorPlain" => Some(Self::Ea1JpnKorPlain),
            "ea1JpnKorPeriod" => Some(Self::Ea1JpnKorPeriod),
            "arabic1Minus" => Some(Self::Arabic1Minus),
            "arabic2Minus" => Some(Self::Arabic2Minus),
            "hebrew2Minus" => Some(Self::Hebrew2Minus),
            "thaiAlphaPeriod" => Some(Self::ThaiAlphaPeriod),
            "thaiAlphaParenR" => Some(Self::ThaiAlphaParenR),
            "thaiAlphaParenBoth" => Some(Self::ThaiAlphaParenBoth),
            "thaiNumPeriod" => Some(Self::ThaiNumPeriod),
            "thaiNumParenR" => Some(Self::ThaiNumParenR),
            "thaiNumParenBoth" => Some(Self::ThaiNumParenBoth),
            "hindiAlphaPeriod" => Some(Self::HindiAlphaPeriod),
            "hindiNumPeriod" => Some(Self::HindiNumPeriod),
            "hindiNumParenR" => Some(Self::HindiNumParenR),
            "hindiAlpha1Period" => Some(Self::HindiAlpha1Period),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextTabAlignType {
    Left,
    Center,
    Right,
    Decimal,
}
impl TextTabAlignType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "l" => Some(Self::Left),
            "ctr" => Some(Self::Center),
            "r" => Some(Self::Right),
            "dec" => Some(Self::Decimal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextUnderlineType {
    None,
    Words,
    Single,
    Double,
    Heavy,
    Dotted,
    DottedHeavy,
    Dash,
    DashHeavy,
    DashLong,
    DashLongHeavy,
    DotDash,
    DotDashHeavy,
    DotDotDash,
    DotDotDashHeavy,
    Wavy,
    WavyHeavy,
    WavyDbl,
}
impl TextUnderlineType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "words" => Some(Self::Words),
            "sng" => Some(Self::Single),
            "dbl" => Some(Self::Double),
            "heavy" => Some(Self::Heavy),
            "dotted" => Some(Self::Dotted),
            "dottedHeavy" => Some(Self::DottedHeavy),
            "dash" => Some(Self::Dash),
            "dashHeavy" => Some(Self::DashHeavy),
            "dashLong" => Some(Self::DashLong),
            "dashLongHeavy" => Some(Self::DashLongHeavy),
            "dotDash" => Some(Self::DotDash),
            "dotDashHeavy" => Some(Self::DotDashHeavy),
            "dotDotDash" => Some(Self::DotDotDash),
            "dotDotDashHeavy" => Some(Self::DotDotDashHeavy),
            "wavy" => Some(Self::Wavy),
            "wavyHeavy" => Some(Self::WavyHeavy),
            "wavyDbl" => Some(Self::WavyDbl),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextStrikeType {
    None,
    Single,
    Double,
}
impl TextStrikeType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "noStrike" => Some(Self::None),
            "sngStrike" => Some(Self::Single),
            "dblStrike" => Some(Self::Double),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TextCapsType {
    None,
    Small,
    All,
}
impl TextCapsType {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "small" => Some(Self::Small),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FontCollectionIndex {
    Major,
    Minor,
    None,
}
impl FontCollectionIndex {
    pub fn try_from_str(s: &str) -> Option<Self> {
        match s {
            "major" => Some(Self::Major),
            "minor" => Some(Self::Minor),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}
