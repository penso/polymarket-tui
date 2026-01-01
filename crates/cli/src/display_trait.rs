use polymarket_tui::rtds::RTDSMessage;

/// Trait for displaying RTDS trade messages
pub trait TradeDisplay: Send + Sync {
    /// Display a trade message
    fn display_trade(&mut self, msg: &RTDSMessage) -> anyhow::Result<()>;
}

/// Simple println-based display (default)
pub struct SimpleDisplay;

impl TradeDisplay for SimpleDisplay {
    fn display_trade(&mut self, msg: &RTDSMessage) -> anyhow::Result<()> {
        use polymarket_tui::RTDSFormatter;

        let formatted = RTDSFormatter::format_message(msg);
        print!("{}", formatted);
        Ok(())
    }
}
