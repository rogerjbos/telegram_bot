use std::{error::Error, path::PathBuf, sync::Arc};

use prettytable::{row, Table};
use teloxide::{
    prelude::*,
    types::{ChatId, ParseMode},
    utils::command::BotCommands,
};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    time::Duration,
};

/// Requests that can be sent to the live trading bot runner.
pub enum BotRequest {
    GetStatus(oneshot::Sender<Result<String, String>>),
}

use crate::{
    error::BotError,
    traits::{SymbolConfig, TradingBot},
};

#[derive(Clone)]
pub struct BotState {
    pub is_running: bool,
    pub notification_level: NotificationLevel,
    pub config_path: Option<String>,
    pub interval_seconds: Option<u64>,
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
            config_path: None,
            interval_seconds: None,
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

pub struct TelegramBotHandler {
    request_tx: mpsc::UnboundedSender<BotRequest>,
}

impl TelegramBotHandler {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<BotRequest>) {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        (Self { request_tx }, request_rx)
    }

    async fn request_status(&self) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(BotRequest::GetStatus(tx))
            .map_err(|_| "Bot runner unavailable".to_string())?;

        rx.await
            .map_err(|_| "Bot runner dropped status channel".to_string())?
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

                    bot.send_message(msg.chat.id, "Trading bot started!")
                        .await?;
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
                let (is_running, notification_level) = {
                    let state = bot_state.lock().await;
                    (state.is_running, state.notification_level.clone())
                };

                let status_msg = if is_running {
                    match self.request_status().await {
                        Ok(status) => format!(
                            "Bot is running.\nNotification level: {:?}\n\n{}",
                            notification_level, status
                        ),
                        Err(err) => format!(
                            "Bot is running, but failed to retrieve status: {}",
                            err
                        ),
                    }
                } else {
                    format!(
                        "Bot is stopped.\nNotification level: {:?}",
                        notification_level
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
                self.handle_add_symbol(&bot, msg.chat.id, data, Arc::clone(&bot_state))
                    .await?;
            }
            Command::RemoveSymbol(symbol) => {
                self.handle_remove_symbol(&bot, msg.chat.id, symbol, Arc::clone(&bot_state))
                    .await?;
            }
            Command::Symbols => {
                self.handle_show_symbols(&bot, msg.chat.id, Arc::clone(&bot_state))
                    .await?;
            }
            Command::Update => {
                match self.request_status().await {
                    Ok(status) => {
                        bot.send_message(msg.chat.id, format!("Current status:\n{}", status))
                            .await?;
                    }
                    Err(err) => {
                        bot.send_message(
                            msg.chat.id,
                            format!(
                                "Unable to retrieve status from running bot: {}",
                                err
                            ),
                        )
                        .await?;
                    }
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
        bot_state: Arc<Mutex<BotState>>,
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

        let config_path = match bot_state.lock().await.config_path.clone() {
            Some(path) => PathBuf::from(path),
            None => {
                bot.send_message(
                    chat_id,
                    "Configuration path is not set. Use /startbot first to initialize.",
                )
                .await?;
                return Ok(());
            }
        };

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
        bot_state: Arc<Mutex<BotState>>,
    ) -> ResponseResult<()> {
        let config_path = match bot_state.lock().await.config_path.clone() {
            Some(path) => PathBuf::from(path),
            None => {
                bot.send_message(
                    chat_id,
                    "Configuration path is not set. Use /startbot first to initialize.",
                )
                .await?;
                return Ok(());
            }
        };

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

    async fn handle_show_symbols(
        &self,
        bot: &Bot,
        chat_id: ChatId,
        bot_state: Arc<Mutex<BotState>>,
    ) -> ResponseResult<()> {
        let config_path = match bot_state.lock().await.config_path.clone() {
            Some(path) => PathBuf::from(path),
            None => {
                bot.send_message(
                    chat_id,
                    "Configuration path is not set. Use /startbot first to initialize.",
                )
                .await?;
                return Ok(());
            }
        };

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
    pub async fn init_and_run_bot<T: TradingBot>(
        bot_state: Arc<Mutex<BotState>>,
        bot: Bot,
        chat_id: ChatId,
        mut request_rx: mpsc::UnboundedReceiver<BotRequest>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Spawn the bot in a new thread to avoid Send issues
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    // Try to initialize the bot
                    let init_result = T::new().await;

                    match init_result {
                        Ok(mut trading_bot) => {
                            let interval_seconds = trading_bot.get_interval_seconds();
                            let config_path = trading_bot.get_config_path().to_string();

                            {
                                let mut state = bot_state.lock().await;
                                state.config_path = Some(config_path);
                                state.interval_seconds = Some(interval_seconds);
                            }

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
                                tokio::select! {
                                    maybe_request = request_rx.recv() => {
                                        match maybe_request {
                                            Some(BotRequest::GetStatus(response_tx)) => {
                                                let status = trading_bot.get_status().await;
                                                let _ = response_tx.send(Ok(status));
                                            }
                                            None => {
                                                println!("Request channel closed, shutting down bot runner");
                                                break;
                                            }
                                        }
                                    }
                                    _ = check_interval.tick() => {
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

                                        match tokio::time::timeout(
                                            Duration::from_secs(60),
                                            trading_bot.execute_strategy(
                                                bot_state.clone(),
                                                bot.clone(),
                                                chat_id,
                                            ),
                                        )
                                        .await
                                        {
                                            Ok(Ok(_)) => {}
                                            Ok(Err(e)) => {
                                                let error_msg = format!("Strategy execution failed: {}", e);
                                                eprintln!("{}", &error_msg);

                                                if let Err(e) = bot.send_message(chat_id, &error_msg).await {
                                                    eprintln!("Error sending error message: {}", e);
                                                }

                                                if let Err(e) = bot
                                                    .send_message(
                                                        chat_id,
                                                        "Stopping and restarting the bot due to error...",
                                                    )
                                                    .await
                                                {
                                                    eprintln!("Error sending restart message: {}", e);
                                                }

                                                {
                                                    let mut state = bot_state.lock().await;
                                                    state.is_running = false;
                                                }

                                                tokio::time::sleep(Duration::from_secs(5)).await;

                                                {
                                                    let mut state = bot_state.lock().await;
                                                    state.is_running = true;
                                                }

                                                if let Err(e) = bot
                                                    .send_message(chat_id, "Bot has been restarted.")
                                                    .await
                                                {
                                                    eprintln!(
                                                        "Error sending restart confirmation message: {}",
                                                        e
                                                    );
                                                }

                                                match T::new().await {
                                                    Ok(new_bot) => {
                                                        let interval_seconds =
                                                            new_bot.get_interval_seconds();
                                                        let config_path =
                                                            new_bot.get_config_path().to_string();

                                                        {
                                                            let mut state = bot_state.lock().await;
                                                            state.config_path = Some(config_path);
                                                            state.interval_seconds = Some(interval_seconds);
                                                        }

                                                        trading_bot = new_bot;
                                                        check_interval = tokio::time::interval(
                                                            Duration::from_secs(interval_seconds),
                                                        );
                                                        check_interval.tick().await;
                                                        if let Err(e) = bot
                                                            .send_message(
                                                                chat_id,
                                                                "Trading bot has been re-initialized.",
                                                            )
                                                            .await
                                                        {
                                                            eprintln!(
                                                                "Error sending re-initialization message: {}",
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
                                                                "Error sending re-initialization error message: {}",
                                                                e
                                                            );
                                                        }

                                                        let mut state = bot_state.lock().await;
                                                        state.is_running = false;
                                                    }
                                                }
                                            }
                                            Err(_) => {
                                                println!("Strategy execution timed out");
                                            }
                                        }
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
const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4096;
const PRE_WRAP_OVERHEAD: usize = "<pre></pre>".len();

fn split_message_chunks(message: &str, max_len: usize) -> Vec<String> {
    if message.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for segment in message.split_inclusive('\n') {
        let seg_len = segment.chars().count();

        if current_len + seg_len <= max_len {
            current.push_str(segment);
            current_len += seg_len;
            continue;
        }

        if !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }

        if seg_len <= max_len {
            current.push_str(segment);
            current_len = seg_len;
        } else {
            let mut buffer = String::new();
            let mut buffer_len = 0usize;

            for ch in segment.chars() {
                if buffer_len == max_len {
                    chunks.push(buffer);
                    buffer = String::new();
                    buffer_len = 0;
                }

                buffer.push(ch);
                buffer_len += 1;
            }

            if !buffer.is_empty() {
                current = buffer;
                current_len = buffer_len;
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

pub async fn send_telegram_notification(
    bot: &Bot,
    chat_id: ChatId,
    level: NotificationLevel,
    current_level: NotificationLevel,
    message: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Only send if the message level is important enough
    if level_is_sufficient(level, current_level) {
        let max_payload_len = TELEGRAM_MAX_MESSAGE_LENGTH.saturating_sub(PRE_WRAP_OVERHEAD);
        let chunks = split_message_chunks(&message, max_payload_len);

        if chunks.is_empty() {
            return Ok(());
        }

        for chunk in chunks {
            let mono_message = format!("<pre>{}</pre>", chunk);
            if let Err(e) = bot
                .send_message(chat_id, mono_message)
                .parse_mode(ParseMode::Html)
                .await
            {
                eprintln!("Failed to send Telegram message: {}", e);
                return Err(Box::new(BotError(format!("Telegram error: {}", e))));
            }
        }

        Ok(())
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
