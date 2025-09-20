use std::{error::Error, fmt};

/// Custom error type for the Telegram bot
#[derive(Debug)]
pub struct BotError(pub String);

impl fmt::Display for BotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for BotError {}

// Safely implement Send and Sync for BotError since it only contains a String
// which is Send + Sync
unsafe impl Send for BotError {}
unsafe impl Sync for BotError {}
