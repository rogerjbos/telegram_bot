use std::error::Error;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use teloxide::{types::ChatId, Bot};

use crate::BotState;

/// Configuration for a trading symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolConfig {
    pub symbol: String,
    pub entry_amount: f64,
    pub exit_amount: f64,
    pub entry_threshold: f64,
    pub exit_threshold: f64,
}

/// Trait that any trading bot must implement to work with the Telegram
/// interface
#[async_trait]
pub trait TradingBot: Send + Sync {
    /// Associated error type for operations
    type Error: std::fmt::Display + Send + Sync + 'static;

    /// Initializes a new trading bot instance.
    ///
    /// # Arguments
    ///
    /// * `interval_seconds` - The interval in seconds between strategy executions
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` when the bot is successfully created
    /// * `Err(Self::Error)` if initialization fails
    async fn new(interval_seconds: u64) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Executes the core trading strategy logic.
    ///
    /// # Arguments
    ///
    /// * `bot_state` - Shared mutable state wrapper controlling bot execution
    /// * `telegram_bot` - Telegram `Bot` instance for sending messages
    /// * `chat_id` - Destination `ChatId` for notifications
    ///
    /// # Returns
    ///
    /// * `Ok(())` when strategy completes without error
    /// * `Err(Self::Error)` if any part of the strategy fails
    async fn execute_strategy(
        &mut self,
        bot_state: std::sync::Arc<tokio::sync::Mutex<BotState>>,
        telegram_bot: Bot,
        chat_id: ChatId,
    ) -> Result<(), Self::Error>;
}

/// Configuration manager trait for handling symbol configurations
#[async_trait]
pub trait ConfigManager {
    /// Associated error type for configuration operations
    type Error: Error + Send + Sync + 'static;

    /// Loads symbol configurations from persistent storage.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<SymbolConfig>)` on success
    /// * `Err(Self::Error)` if loading or parsing fails
    async fn load_symbols(&self) -> Result<Vec<SymbolConfig>, Self::Error>;

    /// Saves symbol configurations to persistent storage.
    ///
    /// # Arguments
    ///
    /// * `symbols` - Vector of `SymbolConfig` to persist
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(Self::Error)` if writing fails
    async fn save_symbols(&self, symbols: Vec<SymbolConfig>) -> Result<(), Self::Error>;

    /// Adds a new symbol configuration.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The `SymbolConfig` to add
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(Self::Error)` if the operation fails
    async fn add_symbol(&self, symbol: SymbolConfig) -> Result<(), Self::Error>;

    /// Removes an existing symbol configuration by name.
    ///
    /// # Arguments
    ///
    /// * `symbol_name` - Name of the symbol to remove
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if the symbol was found and removed
    /// * `Ok(false)` if the symbol was not found
    /// * `Err(Self::Error)` if the operation fails
    async fn remove_symbol(&self, symbol_name: &str) -> Result<bool, Self::Error>;
}
