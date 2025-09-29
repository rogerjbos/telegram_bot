# Telegram Bot Framework 🤖

A generic, reusable Telegram bot library for Rust applications, specifically designed for trading bots but flexible enough for any use case.

## ✨ Features

- **Generic Design**: Works with any application implementing the `TradingBot` trait
- **Async/Await**: Full async support with `tokio` and `async-trait`
- **Command System**: Built-in command parsing and handling
- **State Management**: Thread-safe bot state management
- **Error Handling**: Comprehensive error handling with custom error types
- **Pretty Tables**: Formatted table output for data presentation
- **Notification System**: Multiple notification levels (Info, Warning, Error)

## 🚀 Quick Start

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
telegram-bot = { path = "path/to/telegram-bot" }
# or when published:
# telegram-bot = "0.1.0"
```

## 📖 Usage

### 1. Implement the TradingBot Trait

```rust
use telegram_bot::{TradingBot, BotState};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use teloxide::{Bot, types::ChatId};

struct MyTradingBot {
    // Your bot implementation
}

#[async_trait]
impl TradingBot for MyTradingBot {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn new() -> Result<Self, Self::Error> {
        Ok(MyTradingBot {
            // Initialize your bot
        })
    }

    async fn execute_strategy(
        &mut self,
        bot_state: Arc<Mutex<BotState>>,
        telegram_bot: Bot,
        chat_id: ChatId,
    ) -> Result<(), Self::Error> {
        // Implement your trading strategy
        println!("Executing strategy...");
        Ok(())
    }
}
```

### 2. Initialize and Run the Bot

```rust
use telegram_bot::{TelegramBotHandler, BotState};
use teloxide::{Bot, types::ChatId};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bot = Bot::from_env();
    let chat_id = ChatId(123456789); // Your chat ID
    
    let bot_state = Arc::new(Mutex::new(BotState {
        is_running: true,
        ..Default::default()
    }));

    TelegramBotHandler::<MyTradingBot>::init_and_run_bot(
        bot_state,
        bot,
        chat_id,
    ).await?;

    Ok(())
}
```

## 🏗️ Architecture

### Core Components

#### `TradingBot` Trait
The main trait that your application must implement:

```rust
#[async_trait]
pub trait TradingBot: Send + Sync {
    type Error: std::fmt::Display + Send + Sync + 'static;

    async fn new() -> Result<Self, Self::Error> where Self: Sized;
    
    async fn execute_strategy(
        &mut self,
        bot_state: Arc<Mutex<BotState>>,
        telegram_bot: Bot,
        chat_id: ChatId,
    ) -> Result<(), Self::Error>;
}
```

#### `TelegramBotHandler<T>`
Generic bot handler that manages:
- Command processing
- State management
- Error handling
- User interactions

#### `BotState`
Thread-safe state management:

```rust
#[derive(Debug, Clone, Default)]
pub struct BotState {
    pub is_running: bool,
    pub last_update: Option<DateTime<Utc>>,
    pub notification_level: NotificationLevel,
    pub custom_data: std::collections::HashMap<String, String>,
}
```

#### `NotificationLevel`
Control notification verbosity:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationLevel {
    All,     // All notifications
    Important, // Warnings and errors only
    ErrorsOnly, // Errors only
    None,    // No notifications
}
```

## 📱 Built-in Commands

The framework provides these commands out of the box:

- `/start` - Initialize the bot
- `/help` - Show available commands
- `/status` - Display bot status
- `/execute` - Trigger strategy execution
- `/stop` - Stop the bot
- `/restart` - Restart the bot
- `/notifications <level>` - Set notification level

## 🔧 Customization

### Custom Commands

Extend the command system by modifying the `Command` enum and implementing custom handlers.

### Error Handling

The framework uses a custom `BotError` type with conversion from common error types:

```rust
#[derive(Debug, thiserror::Error)]
pub enum BotError {
    #[error("Telegram error: {0}")]
    Telegram(#[from] teloxide::RequestError),
    
    #[error("Trading error: {0}")]
    Trading(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
}
```

### State Persistence

The `BotState` includes a `custom_data` HashMap for application-specific state:

```rust
let mut state = bot_state.lock().await;
state.custom_data.insert("last_trade".to_string(), "BTC-USD".to_string());
```

## 📊 Table Formatting

Use the built-in table formatting for data presentation:

```rust
use prettytable::{Table, row};

let mut table = Table::new();
table.add_row(row!["Symbol", "Price", "Change"]);
table.add_row(row!["BTC", "$45,000", "+2.5%"]);
// Table automatically formats for Telegram
```

## ⚡ Performance

- **Async/Await**: Non-blocking operations throughout
- **Memory Efficient**: Minimal allocation and smart memory usage
- **Thread Safe**: All components designed for concurrent access
- **Error Recovery**: Graceful handling of network issues and API failures

## 🔒 Security Features

- **Input Validation**: All user inputs are validated
- **Error Sanitization**: Sensitive data excluded from error messages
- **Rate Limiting**: Built-in protection against API rate limits
- **Safe Defaults**: Secure configuration defaults

## 📦 Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `teloxide` | 0.14 | Telegram Bot API |
| `prettytable` | 0.10 | Table formatting |
| `tokio` | 1.0 | Async runtime |
| `serde` | 1.0 | Serialization |
| `async-trait` | 0.1 | Async traits |

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Test a specific module
cargo test bot_state
```

## 📖 Examples

Check the `examples/` directory for complete usage examples:

- `basic_bot.rs` - Minimal bot implementation
- `advanced_bot.rs` - Full-featured bot with custom commands
- `state_management.rs` - Advanced state handling

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test`
5. Format code: `cargo +nightly fmt`
6. Submit a pull request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 Related

- [Teloxide Documentation](https://docs.rs/teloxide/)
- [Telegram Bot API](https://core.telegram.org/bots/api)
- [Async Programming in Rust](https://rust-lang.github.io/async-book/)

---

**Built with ❤️ and Rust** 🦀

*Part of the Rust Trading Bot ecosystem*