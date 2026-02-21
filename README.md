<div align="center">

# 🏛️ Makhzan

### **/mæxˈzæn/** — *مخزن* — "The Vault"

**A dependency injection container that Rust deserves, but never had.**

[![Crates.io](https://img.shields.io/crates/v/makhzan.svg)](https://crates.io/crates/makhzan)
[![docs.rs](https://docs.rs/makhzan/badge.svg)](https://docs.rs/makhzan)
[![CI](https://github.com/YOUR_USERNAME/makhzan/workflows/CI/badge.svg)](https://github.com/YOUR_USERNAME/makhzan/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[Getting Started](#-quick-start) •
[Guide](#-guide) •
[Examples](#-examples) •
[Contributing](#-contributing)

---

*"Your dependencies called. They want to be managed."*

</div>

## 🤔 Why Makhzan?

Let's be honest.

You came to Rust for **safety**, **speed**, and **zero-cost abstractions**.
Then you started building a real application and realized:

> *"Wait... how do I wire 47 services together without losing my mind?"*

You googled "Rust dependency injection" and found:
- 🪦 Abandoned crates with last commit in 2021
- 🧸 Toy projects that handle exactly one use case
- 📖 Blog posts saying "just pass arguments manually lol"

**Makhzan is here to end that suffering.**

```rust
use makhzan::prelude::*;

#[derive(Injectable)]
#[makhzan(Singleton)]
struct Database {
    connection_string: String,
}

#[derive(Injectable)]
#[makhzan(Transient)]
struct UserRepository {
    #[inject]
    db: Arc<Database>,
}

#[derive(Injectable)]
#[makhzan(Scoped)]
struct UserService {
    #[inject]
    repo: Arc<UserRepository>,
    #[inject]
    logger: Arc<dyn Logger>,
}

fn main() -> Result<()> {
    let container = Container::builder()
        .singleton(Database::new("postgres://localhost/app"))
        .bind::<dyn Logger, ConsoleLogger>()
        .auto_register()
        .build()?;

    let service = container.resolve::<UserService>()?;
    // That's it. No 200-line main(). No spaghetti. No tears.
}