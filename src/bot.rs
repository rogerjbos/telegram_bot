use std::{error::Error, path::PathBuf, sync::Arc};

use prettytable::{row, Table};
use teloxide::{
    prelude::*,
    types::{ChatId, ParseMode},
    utils::command::BotCommands,
};
use tokio::{sync::Mutex, time::Duration};

use crate::{
    error::BotError,
    traits::{SymbolConfig, TradingBot},
};

#[derive(Clone)]
pub struct BotState {
    pub is_running: bool,
    pub notification_level: NotificationLevel,
}

/// Notification levels for the Telegram bot
#[derive(Clone, PartialEq, Debug)]
pub enum NotificationLevel {
    All,       // Send all messages
    Important, // Only important updates and errors
    Critical,  // Only critical errors and trade executions
    None,      // No messages
}

impl BotState {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for BotState {
    fn default() -> Self {
        Self {
            is_running: false,
            notification_level: NotificationLevel::Important,
        }
    }
}

#[derive(Debug, BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "start the trading bot.")]
    StartBot,
    #[command(description = "stop the trading bot.")]
    StopBot,
    #[command(description = "check bot status.")]
    Status,
    #[command(description = "set notification level (all/important/critical/none)")]
    Notify(String),
    #[command(description = "request immediate status update")]
    Update,
    #[command(description = "display the contents of symbols configuration.")]
    Symbols,
    #[command(description = "add a new symbol to configuration.")]
    AddSymbol(String), // Pass a single JSON string or delimited string
    #[command(description = "remove a symbol from configuration.")]
    RemoveSymbol(String),
}

pub struct TelegramBotHandler<T: TradingBot> {
    trading_bot: Option<T>,
    config_path: String,
}

impl<T: TradingBot> TelegramBotHandler<T> {
    pub fn new(config_path: String) -> Self {
        Self {
            trading_bot: None,
            config_path,
        }
    }

    /// Handle incoming Telegram commands
    pub async fn handle_command(
        &mut self,
        bot: Bot,
        msg: Message,
        cmd: Command,
        bot_state: Arc<Mutex<BotState>>,
    ) -> ResponseResult<()> {
        match cmd {
            Command::Help => {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
            }
            Command::StartBot => {
                let mut state = bot_state.lock().await;
                if !state.is_running {
                    state.is_running = true;
                    drop(state);

                    // Initialize trading bot if needed
                    if self.trading_bot.is_none() {
                        match T::new().await {
                            Ok(trading_bot) => {
                                self.trading_bot = Some(trading_bot);
                                bot.send_message(msg.chat.id, "Trading bot started successfully!")
                                    .await?;
                            }
                            Err(e) => {
                                let mut state = bot_state.lock().await;
                                state.is_running = false;
                                bot.send_message(
                                    msg.chat.id,
                                    format!("Failed to start bot: {}", e),
                                )
                                .await?;
                            }
                        }
                    } else {
                        bot.send_message(msg.chat.id, "Trading bot started!")
                            .await?;
                    }
                } else {
                    bot.send_message(msg.chat.id, "Bot is already running.")
                        .await?;
                }
            }
            Command::StopBot => {
                let mut state = bot_state.lock().await;
                if state.is_running {
                    state.is_running = false;
                    bot.send_message(msg.chat.id, "Trading bot stopped.")
                        .await?;
                } else {
                    bot.send_message(msg.chat.id, "Bot is not running.").await?;
                }
            }
            Command::Status => {
                let state = bot_state.lock().await;
                let status_msg = if state.is_running {
                    if let Some(ref trading_bot) = self.trading_bot {
                        format!(
                            "Bot is running.\nNotification level: {:?}\n\n{}",
                            state.notification_level,
                            trading_bot.get_status().await
                        )
                    } else {
                        format!(
                            "Bot is running but not initialized.\nNotification level: {:?}",
                            state.notification_level
                        )
                    }
                } else {
                    format!(
                        "Bot is stopped.\nNotification level: {:?}",
                        state.notification_level
                    )
                };
                bot.send_message(msg.chat.id, status_msg).await?;
            }
            Command::Notify(level_str) => {
                let mut state = bot_state.lock().await;
                match level_str.to_lowercase().as_str() {
                    "all" => {
                        state.notification_level = NotificationLevel::All;
                        bot.send_message(msg.chat.id, "Notification level set to All")
                            .await?;
                    }
                    "important" => {
                        state.notification_level = NotificationLevel::Important;
                        bot.send_message(msg.chat.id, "Notification level set to Important")
                            .await?;
                    }
                    "critical" => {
                        state.notification_level = NotificationLevel::Critical;
                        bot.send_message(msg.chat.id, "Notification level set to Critical")
                            .await?;
                    }
                    "none" => {
                        state.notification_level = NotificationLevel::None;
                        bot.send_message(msg.chat.id, "Notifications disabled")
                            .await?;
                    }
                    _ => {
                        bot.send_message(
                            msg.chat.id,
                            "Invalid level. Use: all, important, critical, or none",
                        )
                        .await?;
                    }
                }
            }
            Command::AddSymbol(data) => {
                self.handle_add_symbol(&bot, msg.chat.id, data).await?;
            }
            Command::RemoveSymbol(symbol) => {
                self.handle_remove_symbol(&bot, msg.chat.id, symbol).await?;
            }
            Command::Symbols => {
                self.handle_show_symbols(&bot, msg.chat.id).await?;
            }
            Command::Update => {
                if let Some(ref trading_bot) = self.trading_bot {
                    let status = trading_bot.get_status().await;
                    bot.send_message(msg.chat.id, format!("Current status:\n{}", status))
                        .await?;
                } else {
                    bot.send_message(msg.chat.id, "Trading bot is not initialized.")
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_add_symbol(
        &self,
        bot: &Bot,
        chat_id: ChatId,
        data: String,
    ) -> ResponseResult<()> {
        let parts: Vec<&str> = data.split(',').collect();
        if parts.len() != 5 {
            bot.send_message(
                chat_id,
                "Invalid format. Use: /addsymbol \
                 SYMBOL,ENTRY_AMOUNT,EXIT_AMOUNT,ENTRY_THRESHOLD,EXIT_THRESHOLD",
            )
            .await?;
            return Ok(());
        }

        let symbol = parts[0].trim().to_string();
        let entry_amount: f64 = parts[1].trim().parse().unwrap_or(0.0);
        let exit_amount: f64 = parts[2].trim().parse().unwrap_or(0.0);
        let entry_threshold: f64 = parts[3].trim().parse().unwrap_or(0.0);
        let exit_threshold: f64 = parts[4].trim().parse().unwrap_or(0.0);

        let config_path = PathBuf::from(&self.config_path);

        // Read the current file content
        let file_content = tokio::fs::read_to_string(config_path.clone()).await;

        match file_content {
            Ok(content) => {
                let mut symbols: Vec<SymbolConfig> = match serde_json::from_str(&content) {
                    Ok(json) => json,
                    Err(_) => {
                        bot.send_message(chat_id, "Failed to parse symbols configuration.")
                            .await?;
                        return Ok(());
                    }
                };

                // Add the new symbol
                let new_symbol = SymbolConfig {
                    symbol: symbol.clone(),
                    entry_amount,
                    exit_amount,
                    entry_threshold,
                    exit_threshold,
                };
                symbols.push(new_symbol);

                // Write the updated content back to the file
                if tokio::fs::write(config_path, serde_json::to_string_pretty(&symbols).unwrap())
                    .await
                    .is_err()
                {
                    bot.send_message(chat_id, "Failed to update symbols configuration.")
                        .await?;
                    return Ok(());
                }

                bot.send_message(chat_id, format!("Symbol '{}' added successfully.", symbol))
                    .await?;
            }
            Err(_) => {
                bot.send_message(
                    chat_id,
                    "Failed to read symbols configuration. Ensure the file exists.",
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn handle_remove_symbol(
        &self,
        bot: &Bot,
        chat_id: ChatId,
        symbol: String,
    ) -> ResponseResult<()> {
        let config_path = PathBuf::from(&self.config_path);

        // Read the current file content
        let file_content = tokio::fs::read_to_string(config_path.clone()).await;

        match file_content {
            Ok(content) => {
                let mut symbols: Vec<SymbolConfig> = match serde_json::from_str(&content) {
                    Ok(json) => json,
                    Err(_) => {
                        bot.send_message(chat_id, "Failed to parse symbols configuration.")
                            .await?;
                        return Ok(());
                    }
                };

                // Remove the symbol
                let original_len = symbols.len();
                symbols.retain(|s| s.symbol != symbol);

                if symbols.len() == original_len {
                    bot.send_message(chat_id, format!("Symbol '{}' not found.", symbol))
                        .await?;
                    return Ok(());
                }

                // Write the updated content back to the file
                if tokio::fs::write(config_path, serde_json::to_string_pretty(&symbols).unwrap())
                    .await
                    .is_err()
                {
                    bot.send_message(chat_id, "Failed to update symbols configuration.")
                        .await?;
                    return Ok(());
                }

                bot.send_message(
                    chat_id,
                    format!("Symbol '{}' removed successfully.", symbol),
                )
                .await?;
            }
            Err(_) => {
                bot.send_message(
                    chat_id,
                    "Failed to read symbols configuration. Ensure the file exists.",
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn handle_show_symbols(&self, bot: &Bot, chat_id: ChatId) -> ResponseResult<()> {
        let config_path = PathBuf::from(&self.config_path);

        // Read the file
        let file_content = tokio::fs::read_to_string(config_path).await;

        match file_content {
            Ok(content) => {
                // Parse the JSON
                let symbols: Vec<SymbolConfig> = match serde_json::from_str(&content) {
                    Ok(json) => json,
                    Err(_) => {
                        bot.send_message(chat_id, "Failed to parse symbols configuration.")
                            .await?;
                        return Ok(());
                    }
                };

                // Create a table
                let mut table = Table::new();
                table.add_row(row![
                    "Symbol",
                    "Entry Amount",
                    "Exit Amount",
                    "Entry Threshold",
                    "Exit Threshold"
                ]);

                // Populate the table
                for symbol in &symbols {
                    table.add_row(row![
                        symbol.symbol,
                        format!("{:.2}", symbol.entry_amount),
                        format!("{:.2}", symbol.exit_amount),
                        format!("{:.2}", symbol.entry_threshold),
                        format!("{:.2}", symbol.exit_threshold)
                    ]);
                }

                // Convert the table to a string
                let table_string = format!("```\n{}\n```", table.to_string());

                // Send the table as a message
                bot.send_message(chat_id, table_string)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
            }
            Err(_) => {
                bot.send_message(
                    chat_id,
                    "Failed to read symbols configuration. Ensure the file exists.",
                )
                .await?;
            }
        }

        Ok(())
    }

    /// Initialize and run the trading bot in a separate thread
    pub async fn init_and_run_bot(
        bot_state: Arc<Mutex<BotState>>,
        bot: Bot,
        chat_id: ChatId,
        interval_seconds: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Spawn the bot in a new thread to avoid Send issues
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    // Try to initialize the bot
                    let init_result = T::new().await;

                    match init_result {
                        Ok(mut trading_bot) => {
                            // Send confirmation message
                            if let Err(e) = bot
                                .send_message(
                                    chat_id,
                                    "Trading bot has initialized and is now running.",
                                )
                                .await
                            {
                                eprintln!("Error sending message: {}", e);
                            }

                            let mut check_interval =
                                tokio::time::interval(Duration::from_secs(interval_seconds));

                            // First tick is consumed
                            check_interval.tick().await;

                            loop {
                                // Check if we should stop
                                let should_run = {
                                    let state = bot_state.lock().await;
                                    state.is_running
                                };

                                if !should_run {
                                    println!("Stop flag detected, shutting down bot");
                                    if let Err(e) = bot
                                        .send_message(chat_id, "Trading bot has been stopped.")
                                        .await
                                    {
                                        eprintln!("Error sending stop message: {}", e);
                                    }
                                    break;
                                }

                                // Always wait for the check interval
                                check_interval.tick().await;

                                // Execute strategy with timeout protection
                                match tokio::time::timeout(
                                    Duration::from_secs(60), // Timeout after 60 seconds
                                    trading_bot.execute_strategy(
                                        bot_state.clone(),
                                        bot.clone(),
                                        chat_id,
                                    ),
                                )
                                .await
                                {
                                    Ok(Ok(_)) => {
                                        // Strategy execution completed
                                        // successfully
                                    }
                                    Ok(Err(e)) => {
                                        let error_msg = format!("Strategy execution failed: {}", e);
                                        eprintln!("{}", &error_msg);

                                        // Send error message
                                        if let Err(e) = bot.send_message(chat_id, &error_msg).await
                                        {
                                            eprintln!("Error sending error message: {}", e);
                                        }

                                        // Send message about restarting the bot
                                        if let Err(e) = bot
                                            .send_message(
                                                chat_id,
                                                "Stopping and restarting the bot due to error...",
                                            )
                                            .await
                                        {
                                            eprintln!("Error sending restart message: {}", e);
                                        }

                                        // Briefly stop the bot
                                        {
                                            let mut state = bot_state.lock().await;
                                            state.is_running = false;
                                        }

                                        // Wait a moment before restarting
                                        tokio::time::sleep(Duration::from_secs(5)).await;

                                        // Restart the bot
                                        {
                                            let mut state = bot_state.lock().await;
                                            state.is_running = true;
                                        }

                                        // Send confirmation of restart
                                        if let Err(e) = bot
                                            .send_message(chat_id, "Bot has been restarted.")
                                            .await
                                        {
                                            eprintln!(
                                                "Error sending restart confirmation message: {}",
                                                e
                                            );
                                        }

                                        // Re-initialize the bot with a fresh instance
                                        match T::new().await {
                                            Ok(new_bot) => {
                                                trading_bot = new_bot; // Replace with the new instance
                                                if let Err(e) = bot
                                                    .send_message(
                                                        chat_id,
                                                        "Trading bot has been re-initialized.",
                                                    )
                                                    .await
                                                {
                                                    eprintln!(
                                                        "Error sending re-initialization message: \
                                                         {}",
                                                        e
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                let init_error_msg =
                                                    format!("Failed to re-initialize bot: {}", e);
                                                eprintln!("{}", &init_error_msg);

                                                if let Err(e) =
                                                    bot.send_message(chat_id, &init_error_msg).await
                                                {
                                                    eprintln!(
                                                        "Error sending re-initialization error \
                                                         message: {}",
                                                        e
                                                    );
                                                }

                                                // Set the bot to stopped state
                                                let mut state = bot_state.lock().await;
                                                state.is_running = false;
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        println!("Strategy execution timed out");
                                        // You might want to send a message or
                                        // handle the timeout
                                        // differently
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            // Safely handle error
                            let error_msg = format!("Failed to initialize bot: {}", e);
                            eprintln!("{}", &error_msg);

                            if let Err(e) = bot.send_message(chat_id, &error_msg).await {
                                eprintln!("Error sending initialization error message: {}", e);
                            }

                            // Reset the running state
                            let mut state = bot_state.lock().await;
                            state.is_running = false;
                        }
                    }
                });
        });
        // Don't wait for thread completion - just return success
        Ok(())
    }
}

/// Create a helper function for sending messages
pub async fn send_telegram_notification(
    bot: &Bot,
    chat_id: ChatId,
    level: NotificationLevel,
    current_level: NotificationLevel,
    message: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Only send if the message level is important enough
    if level_is_sufficient(level, current_level) {
        let mono_message = format!("<pre>{}</pre>", message);
        match bot
            .send_message(chat_id, mono_message)
            .parse_mode(ParseMode::Html)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("Failed to send Telegram message: {}", e);
                Err(Box::new(BotError(format!("Telegram error: {}", e))))
            }
        }
    } else {
        Ok(())
    }
}

/// Helper to check if notification level is sufficient
fn level_is_sufficient(msg_level: NotificationLevel, current_level: NotificationLevel) -> bool {
    match current_level {
        NotificationLevel::None => false,
        NotificationLevel::Critical => msg_level == NotificationLevel::Critical,
        NotificationLevel::Important => {
            matches!(
                msg_level,
                NotificationLevel::Critical | NotificationLevel::Important
            )
        }
        NotificationLevel::All => true,
    }
}
