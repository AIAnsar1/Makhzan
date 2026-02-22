//! Basic example of Makhzan DI container.

use makhzan_container::container::{Container, ResolverApi};
use makhzan_container::error::Result;
use std::sync::Arc;

// === Define your traits and types ===

trait Logger: Send + Sync {
    fn log(&self, msg: &str);
}

#[derive(Clone)]
struct ConsoleLogger;

impl Logger for ConsoleLogger {
    fn log(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

#[derive(Clone)]
struct Config {
    database_url: String,
    debug: bool,
}

struct Database {
    url: String,
    logger: Arc<dyn Logger>,
}

impl Database {
    fn query(&self, sql: &str) -> String {
        self.logger.log(&format!("Executing: {sql}"));
        format!("Results from {}", self.url)
    }
}

struct UserRepository {
    db: Arc<Database>,
}

impl UserRepository {
    fn find_user(&self, id: u64) -> String {
        self.db.query(&format!("SELECT * FROM users WHERE id = {id}"))
    }
}

struct UserService {
    repo: Arc<UserRepository>,
    logger: Arc<dyn Logger>,
}

impl UserService {
    fn get_user(&self, id: u64) -> String {
        self.logger.log(&format!("Getting user {id}"));
        self.repo.find_user(id)
    }
}

fn main() -> Result<()> {
    // Initialize tracing (logging)
    tracing_subscriber::fmt()
        .with_env_filter("makhzan=debug")
        .init();

    // Build the container
    let container = Container::builder()
        // Config — singleton value (already created)
        .singleton_value(Config {
            database_url: "postgres://localhost/myapp".to_string(),
            debug: true,
        })
        // Logger — singleton
        .singleton_with::<Arc<dyn Logger>>(|_| {
            Ok(Arc::new(ConsoleLogger) as Arc<dyn Logger>)
        })
        // Database — singleton (depends on Config + Logger)
        .singleton_with::<Arc<Database>>(|r| {
            let config: Config = r.resolve()?;
            let logger: Arc<dyn Logger> = r.resolve()?;
            Ok(Arc::new(Database {
                url: config.database_url,
                logger,
            }))
        })
        // UserRepository — scoped (one per request)
        .scoped_with::<Arc<UserRepository>>(|r| {
            let db: Arc<Database> = r.resolve()?;
            Ok(Arc::new(UserRepository { db }))
        })
        // UserService — transient (new each time)
        .transient_with::<UserService>(|r| {
            let repo: Arc<UserRepository> = r.resolve()?;
            let logger: Arc<dyn Logger> = r.resolve()?;
            Ok(UserService { repo, logger })
        })
        .build()?;

    println!("✅ Container built successfully!");
    println!("{container:?}");

    // === Resolve from root container ===
    let config: Config = container.resolve()?;
    println!("📋 Config: database_url={}, debug={}", config.database_url, config.debug);

    // === Create a scope (e.g., for an HTTP request) ===
    {
        let scope = container.create_scope();

        let service: UserService = scope.resolve()?;
        let result = service.get_user(42);
        println!("👤 {result}");

        // Resolve again in same scope — UserRepository is reused
        let service2: UserService = scope.resolve()?;
        let result2 = service2.get_user(7);
        println!("👤 {result2}");
    }
    // scope dropped — all Scoped instances cleaned up

    println!("\n🎉 Everything works!");
    Ok(())
}
