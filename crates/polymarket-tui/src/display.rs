use crate::gamma::MarketInfo;
use crate::rtds::RTDSMessage;
use crate::websocket::{OrderUpdate, OrderbookUpdate, PriceUpdate, TradeUpdate, WebSocketMessage};
use chrono::DateTime;
use colored::*;

pub struct MarketUpdateFormatter;

pub struct RTDSFormatter;

impl MarketUpdateFormatter {
    pub fn format_message(msg: &WebSocketMessage, market_info: Option<&MarketInfo>) -> String {
        match msg {
            WebSocketMessage::Orderbook(update) => Self::format_orderbook(update, market_info),
            WebSocketMessage::Trade(update) => Self::format_trade(update, market_info),
            WebSocketMessage::Order(update) => Self::format_order(update, market_info),
            WebSocketMessage::Price(update) => Self::format_price(update, market_info),
            WebSocketMessage::Error(err) => {
                format!("{} {}", "ERROR".red().bold(), err.error.red())
            }
            WebSocketMessage::Subscribed(sub) => {
                format!("{} {}", "✓ SUBSCRIBED".green().bold(), sub.message.green())
            }
            WebSocketMessage::Unknown => "Unknown message type".yellow().to_string(),
        }
    }

    fn format_orderbook(update: &OrderbookUpdate, market_info: Option<&MarketInfo>) -> String {
        let title = market_info
            .map(|m| format!("{} - {}", m.event_title.bold(), m.market_question))
            .unwrap_or_else(|| "Unknown Market".to_string());

        let timestamp = update
            .timestamp
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0))
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());

        let mut output = format!(
            "\n{} {} {}\n",
            "📊 ORDERBOOK".cyan().bold(),
            timestamp.dimmed(),
            title
        );

        // Best bid/ask
        if let Some(best_bid) = update.bids.first() {
            output.push_str(&format!(
                "  {} {} @ {}\n",
                "BID".green().bold(),
                best_bid.size.bright_green(),
                best_bid.price.bright_green()
            ));
        }

        if let Some(best_ask) = update.asks.first() {
            output.push_str(&format!(
                "  {} {} @ {}\n",
                "ASK".red().bold(),
                best_ask.size.bright_red(),
                best_ask.price.bright_red()
            ));
        }

        // Spread
        if let (Some(bid), Some(ask)) = (update.bids.first(), update.asks.first()) {
            if let (Ok(bid_price), Ok(ask_price)) =
                (bid.price.parse::<f64>(), ask.price.parse::<f64>())
            {
                let spread = ask_price - bid_price;
                let spread_pct = (spread / bid_price) * 100.0;
                output.push_str(&format!(
                    "  {} {:.4} ({:.2}%)\n",
                    "SPREAD".yellow(),
                    spread,
                    spread_pct
                ));
            }
        }

        output
    }

    fn format_trade(update: &TradeUpdate, market_info: Option<&MarketInfo>) -> String {
        let title = market_info
            .map(|m| format!("{} - {}", m.event_title.bold(), m.market_question))
            .unwrap_or_else(|| "Unknown Market".to_string());

        let timestamp = update
            .timestamp
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0))
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());

        let side_color = if update.side == "buy" {
            "🟢 BUY".green().bold()
        } else {
            "🔴 SELL".red().bold()
        };

        format!(
            "\n{} {} {} {} @ {} - {}\n",
            "💸 TRADE".bright_yellow().bold(),
            timestamp.dimmed(),
            side_color,
            update.size.bright_white(),
            update.price.bright_white().bold(),
            title
        )
    }

    fn format_order(update: &OrderUpdate, market_info: Option<&MarketInfo>) -> String {
        let title = market_info
            .map(|m| format!("{} - {}", m.event_title.bold(), m.market_question))
            .unwrap_or_else(|| "Unknown Market".to_string());

        let timestamp = update
            .timestamp
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0))
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());

        let status_color = match update.status.as_str() {
            "open" => "OPEN".green(),
            "filled" => "FILLED".bright_green(),
            "cancelled" => "CANCELLED".red(),
            _ => update.status.as_str().yellow(),
        };

        format!(
            "\n{} {} {} {} @ {} - {} - {}\n",
            "📝 ORDER".blue().bold(),
            timestamp.dimmed(),
            status_color.bold(),
            update.size,
            update.price,
            update.side.to_uppercase(),
            title
        )
    }

    fn format_price(update: &PriceUpdate, market_info: Option<&MarketInfo>) -> String {
        let title = market_info
            .map(|m| format!("{} - {}", m.event_title.bold(), m.market_question))
            .unwrap_or_else(|| "Unknown Market".to_string());

        let timestamp = update
            .timestamp
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0))
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());

        format!(
            "\n{} {} Price: {} - {}\n",
            "💰 PRICE".magenta().bold(),
            timestamp.dimmed(),
            update.price.bright_white().bold(),
            title
        )
    }
}

impl RTDSFormatter {
    pub fn format_message(msg: &RTDSMessage) -> String {
        let timestamp = DateTime::from_timestamp(msg.payload.timestamp, 0)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "now".to_string());

        let side_color = if msg.payload.side == "BUY" {
            "🟢 BUY".green().bold()
        } else {
            "🔴 SELL".red().bold()
        };

        let outcome_color = if msg.payload.outcome == "Yes" {
            msg.payload.outcome.bright_green()
        } else {
            msg.payload.outcome.bright_red()
        };

        // Round shares to 2 decimal places
        let rounded_shares = (msg.payload.size * 100.0).round() / 100.0;

        // Calculate total value in dollars
        let total_value = msg.payload.price * msg.payload.size;

        format!(
            "\n{} {} {} {} @ ${:.4} ({} shares, ${:.2}) - {} - {}\n  User: {} ({})\n",
            "💸 TRADE".bright_yellow().bold(),
            timestamp.dimmed(),
            side_color,
            outcome_color.bold(),
            msg.payload.price,
            rounded_shares,
            total_value,
            msg.payload.title.bold(),
            msg.payload.event_slug.dimmed(),
            msg.payload.name.bright_white(),
            msg.payload.pseudonym.dimmed()
        )
    }
}
