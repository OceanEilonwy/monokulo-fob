//! Prices, in words.

use fob_price::{CurrencyHeading, TransportNote};

use crate::{Device, device};

/// What choosing a way of asking for prices means.
pub fn transport_note(note: TransportNote) -> String {
    transport_note_on(device(), note)
}

fn transport_note_on(device: Device, note: TransportNote) -> String {
    let device = device.noun();
    match note {
        TransportNote::SeesAddress => format!("The provider sees this {device}'s IP address"),
        TransportNote::NotAnswered { provider, transport } => format!("{provider} doesn't answer {}", transport.text()),
        TransportNote::NotBuilt => format!("Hides this {device}'s IP address · not built into Fob yet"),
        TransportNote::HidesAddress => format!("Hides this {device}'s IP address; slower"),
    }
}

/// A part of the "Default currency" list: "MONERO", "THIS PHONE'S", "ALL 45".
pub fn currency_heading(heading: CurrencyHeading) -> String {
    currency_heading_on(device(), heading)
}

fn currency_heading_on(device: Device, heading: CurrencyHeading) -> String {
    match heading {
        CurrencyHeading::Monero => "MONERO".into(),
        CurrencyHeading::ThisDevice => format!("THIS {}’S", device.noun().to_uppercase()),
        CurrencyHeading::All { count } => format!("ALL {count}"),
        CurrencyHeading::Found { count, of } => format!("{count} OF {of}"),
    }
}

#[cfg(test)]
mod tests {
    use fob_price::Transport;

    use super::*;

    #[test]
    fn prices_say_what_they_always_said_on_a_phone() {
        let on = |note| transport_note_on(Device::Phone, note);
        assert_eq!(on(TransportNote::SeesAddress), "The provider sees this phone's IP address");
        assert_eq!(on(TransportNote::NotAnswered { provider: "Kraken", transport: Transport::Tor }), "Kraken doesn't answer over Tor");
        assert_eq!(on(TransportNote::NotBuilt), "Hides this phone's IP address · not built into Fob yet");
        assert_eq!(on(TransportNote::HidesAddress), "Hides this phone's IP address; slower");
        let heading = |h| currency_heading_on(Device::Phone, h);
        assert_eq!(heading(CurrencyHeading::Monero), "MONERO");
        assert_eq!(heading(CurrencyHeading::ThisDevice), "THIS PHONE’S");
        assert_eq!(heading(CurrencyHeading::All { count: 45 }), "ALL 45");
        assert_eq!(heading(CurrencyHeading::Found { count: 3, of: 45 }), "3 OF 45");
    }

    #[test]
    fn on_a_computer_they_name_the_computer() {
        assert_eq!(transport_note_on(Device::Computer, TransportNote::SeesAddress), "The provider sees this computer's IP address");
        assert_eq!(currency_heading_on(Device::Computer, CurrencyHeading::ThisDevice), "THIS COMPUTER’S");
    }
}
