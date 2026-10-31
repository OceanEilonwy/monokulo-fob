//! The currencies Fob can show amounts in, baked in: CoinGecko's money
//! list as checked on 2026-10-31, 45 of them. Which providers price each is
//! up to the providers ([`super::Provider::prices`]); this is how each is
//! written.

use unicode_normalization::UnicodeNormalization;

/// One currency: how it's named and written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    /// ISO 4217: "AUD".
    pub code: &'static str,
    /// "Australian dollar".
    pub name: &'static str,
    /// "Australian dollars", as in "Amount in Australian dollars".
    pub plural: &'static str,
    /// Written before the amount, unmistakable among these 45 ("A$", "C$",
    /// "¥" for the yen, "CN¥" for the yuan); none where no short sign is
    /// clear ("kr" is three currencies, "zł" goes after), which is written
    /// by its code after the amount.
    pub symbol: Option<&'static str>,
    /// Digits after the point: ISO 4217's, except 0 for the rupiah and the
    /// forint, whose smaller units nobody writes.
    pub decimals: u32,
}

const fn c(code: &'static str, name: &'static str, plural: &'static str, symbol: Option<&'static str>, decimals: u32) -> Info {
    Info { code, name, plural, symbol, decimals }
}

/// By code.
pub const TABLE: [Info; 45] = [
    c("AED", "UAE dirham", "UAE dirhams", None, 2),
    c("ARS", "Argentine peso", "Argentine pesos", None, 2),
    c("AUD", "Australian dollar", "Australian dollars", Some("A$"), 2),
    c("BDT", "Bangladeshi taka", "Bangladeshi taka", Some("৳"), 2),
    c("BHD", "Bahraini dinar", "Bahraini dinars", None, 3),
    c("BMD", "Bermudian dollar", "Bermudian dollars", None, 2),
    c("BRL", "Brazilian real", "Brazilian reais", Some("R$"), 2),
    c("CAD", "Canadian dollar", "Canadian dollars", Some("C$"), 2),
    c("CHF", "Swiss franc", "Swiss francs", None, 2),
    c("CLP", "Chilean peso", "Chilean pesos", None, 0),
    c("CNY", "Chinese yuan", "Chinese yuan", Some("CN¥"), 2),
    c("CZK", "Czech koruna", "Czech korunas", None, 2),
    c("DKK", "Danish krone", "Danish kroner", None, 2),
    c("EUR", "Euro", "euros", Some("€"), 2),
    c("GBP", "British pound", "British pounds", Some("£"), 2),
    c("GEL", "Georgian lari", "Georgian lari", Some("₾"), 2),
    c("HKD", "Hong Kong dollar", "Hong Kong dollars", Some("HK$"), 2),
    c("HUF", "Hungarian forint", "Hungarian forints", None, 0),
    c("IDR", "Indonesian rupiah", "Indonesian rupiahs", Some("Rp"), 0),
    c("ILS", "Israeli new shekel", "Israeli new shekels", Some("₪"), 2),
    c("INR", "Indian rupee", "Indian rupees", Some("₹"), 2),
    c("JPY", "Japanese yen", "Japanese yen", Some("¥"), 0),
    c("KRW", "South Korean won", "South Korean won", Some("₩"), 0),
    c("KWD", "Kuwaiti dinar", "Kuwaiti dinars", None, 3),
    c("LKR", "Sri Lankan rupee", "Sri Lankan rupees", None, 2),
    c("MMK", "Myanmar kyat", "Myanmar kyats", None, 2),
    c("MXN", "Mexican peso", "Mexican pesos", Some("MX$"), 2),
    c("MYR", "Malaysian ringgit", "Malaysian ringgits", Some("RM"), 2),
    c("NGN", "Nigerian naira", "Nigerian naira", Some("₦"), 2),
    c("NOK", "Norwegian krone", "Norwegian kroner", None, 2),
    c("NZD", "New Zealand dollar", "New Zealand dollars", Some("NZ$"), 2),
    c("PHP", "Philippine peso", "Philippine pesos", Some("₱"), 2),
    c("PKR", "Pakistani rupee", "Pakistani rupees", None, 2),
    c("PLN", "Polish złoty", "Polish złotys", None, 2),
    c("RUB", "Russian ruble", "Russian rubles", Some("₽"), 2),
    c("SAR", "Saudi riyal", "Saudi riyals", None, 2),
    c("SEK", "Swedish krona", "Swedish kronor", None, 2),
    c("SGD", "Singapore dollar", "Singapore dollars", Some("S$"), 2),
    c("THB", "Thai baht", "Thai baht", Some("฿"), 2),
    c("TRY", "Turkish lira", "Turkish lira", Some("₺"), 2),
    c("TWD", "New Taiwan dollar", "New Taiwan dollars", Some("NT$"), 2),
    c("UAH", "Ukrainian hryvnia", "Ukrainian hryvnias", Some("₴"), 2),
    c("USD", "US dollar", "US dollars", Some("$"), 2),
    c("VND", "Vietnamese dong", "Vietnamese dong", Some("₫"), 0),
    c("ZAR", "South African rand", "South African rand", Some("R"), 2),
];

/// Every code in [`TABLE`], in its order.
pub const CODES: [&str; 45] = {
    let mut out = [""; 45];
    let mut i = 0;
    while i < TABLE.len() {
        out[i] = TABLE[i].code;
        i += 1;
    }
    out
};

/// The currency with `code` (upper case), if it's one of the 45.
pub fn find(code: &str) -> Option<&'static Info> {
    TABLE.iter().find(|c| c.code == code)
}

/// The 45, A–Z by name.
pub fn by_name() -> Vec<&'static Info> {
    let mut all: Vec<&Info> = TABLE.iter().collect();
    all.sort_by_key(|c| fold(c.name));
    all
}

/// Text for matching: lower case, accents off ("złoty" is "zloty").
pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ł' => 'l',
            'ø' => 'o',
            'đ' => 'd',
            c => c,
        })
        .collect()
}

/// Whether a search for `query` finds `info`: by name, code or symbol,
/// whatever the case and accents. A blank query finds everything.
pub fn matches(info: &Info, query: &str) -> bool {
    let query = fold(query.trim());
    query.is_empty()
        || fold(info.name).contains(&query)
        || fold(info.plural).contains(&query)
        || fold(info.code).contains(&query)
        || info.symbol.is_some_and(|s| fold(s).contains(&query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_45_currencies_each_once() {
        let mut codes = CODES.to_vec();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 45);
        assert_eq!(codes, CODES, "kept by code");
        assert!(CODES.iter().all(|c| c.len() == 3 && c.chars().all(|c| c.is_ascii_uppercase())));
        let mut names: Vec<&str> = TABLE.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 45);
    }

    #[test]
    fn symbols_are_unmistakable() {
        let symbols: Vec<&str> = TABLE.iter().filter_map(|c| c.symbol).collect();
        let mut unique = symbols.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), symbols.len(), "no two currencies share a sign");
        // None is a code, nor ɱ (XMR's).
        assert!(symbols.iter().all(|s| !CODES.contains(s) && *s != crate::amount::SIGN));
        assert_eq!(find("USD").unwrap().symbol, Some("$"));
        assert_eq!(find("AUD").unwrap().symbol, Some("A$"));
        assert_eq!(find("CHF").unwrap().symbol, None);
    }

    #[test]
    fn decimals_follow_iso_4217_or_what_people_write() {
        for (code, decimals) in [("JPY", 0), ("KRW", 0), ("VND", 0), ("CLP", 0), ("IDR", 0), ("HUF", 0), ("BHD", 3), ("KWD", 3), ("USD", 2), ("TWD", 2)] {
            assert_eq!(find(code).unwrap().decimals, decimals, "{code}");
        }
    }

    #[test]
    fn searching_ignores_case_and_accents() {
        let found = |q: &str| by_name().into_iter().filter(|c| matches(c, q)).map(|c| c.code).collect::<Vec<_>>();
        assert_eq!(found("zloty"), ["PLN"]);
        assert_eq!(found("ZŁOTY"), ["PLN"]);
        assert_eq!(found("nzd"), ["NZD"]);
        assert_eq!(found("A$"), ["AUD"]);
        assert_eq!(found("¥"), ["CNY", "JPY"]);
        assert_eq!(found("krone"), ["DKK", "NOK"]);
        assert_eq!(found("euros"), ["EUR"]);
        assert_eq!(found(" "), by_name().iter().map(|c| c.code).collect::<Vec<_>>());
        assert!(found("xyz").is_empty());
    }

    #[test]
    fn by_name_runs_a_to_z() {
        let names: Vec<&str> = by_name().iter().map(|c| c.name).collect();
        assert_eq!(names.first(), Some(&"Argentine peso"));
        assert_eq!(names.last(), Some(&"Vietnamese dong"));
        assert!(names.windows(2).all(|w| fold(w[0]) <= fold(w[1])));
    }
}
