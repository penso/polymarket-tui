use polymarket_bot::rtds::RTDSMessage;

/// Trait for displaying RTDS trade messages
pub trait TradeDisplay: Send + Sync {
    /// Initialize the display (called once at startup)
    fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Display a trade message
    fn display_trade(&mut self, msg: &RTDSMessage) -> anyhow::Result<()>;

    /// Cleanup the display (called on exit)
    fn cleanup(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Simple println-based display (default)
pub struct SimpleDisplay;

impl TradeDisplay for SimpleDisplay {
    fn display_trade(&mut self, msg: &RTDSMessage) -> anyhow::Result<()> {
        use polymarket_bot::RTDSFormatter;
        
        let formatted = RTDSFormatter::format_message(msg);
        print!("{}", formatted);
        Ok(())
    }
}

