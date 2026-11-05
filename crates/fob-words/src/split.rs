//! Split up, in words.

pub const TITLE: &str = "Split up";

/// Where a split pays, on the confirm screen.
pub const TO: &str = "This wallet";

/// "1 coin", "12 coins".
pub fn coins(n: u32) -> String {
    if n == 1 { "1 coin".into() } else { format!("{n} coins") }
}
