# Rust Foundations for Kubernetes DevOps Engineering

**Project:** Kubernetes Cluster Observer & Controller (Rust)\
**Level:** Beginner → Professional Foundations\
**Last Updated:** 2026-02-20

------------------------------------------------------------------------

# 1️⃣ Rust Theory --- The Minimum You Must Understand

## 1.1 Why Rust Is Different

Rust guarantees:

-   Memory safety\
-   No garbage collector\
-   No data races\
-   High performance

It achieves this using a strict compile-time model centered on
**ownership and borrowing**.

------------------------------------------------------------------------

## The Ownership Model

This is the most important concept in Rust.

### Core Rules

-   Every value has exactly one owner.
-   When the owner goes out of scope → the value is dropped.
-   You cannot have two mutable references at the same time.

### Example

``` rust
let s = String::from("hello");
```

Here:

-   `s` owns the `String`.
-   When `s` goes out of scope → memory is freed automatically.

### Ownership Move

``` rust
let s1 = String::from("hello");
let s2 = s1;
```

Now:

-   `s1` is invalid.
-   Ownership moved to `s2`.

This prevents double-free errors and undefined behavior.

------------------------------------------------------------------------

## 1.2 Borrowing

Instead of transferring ownership:

``` rust
fn print_string(s: &String) {
    println!("{}", s);
}
```

`&String` is a reference (borrow).

### Borrowing Rules

-   Many immutable borrows allowed.
-   Only one mutable borrow allowed.
-   Cannot mix mutable + immutable at the same time.

This prevents race conditions at compile time.

------------------------------------------------------------------------

## 1.3 Structs

Similar to Go structs or Python classes.

``` rust
struct AppConfig {
    name: String,
    version: String,
}
```

------------------------------------------------------------------------

## 1.4 Enums

Rust enums are extremely powerful.

``` rust
enum Command {
    Version,
    Check,
}
```

Enums can also hold data.

------------------------------------------------------------------------

## 1.5 Result and Error Handling

Rust does NOT use exceptions.

Instead:

``` rust
fn do_something() -> Result<String, String> {
    Ok("success".to_string())
}
```

Or:

``` rust
Err("error message".to_string())
```

Errors must be handled explicitly, increasing reliability in production
systems.

------------------------------------------------------------------------

# 2️⃣ Installing Rust Properly

On Linux:

``` bash
curl https://sh.rustup.rs -sSf | sh
```

Reload shell:

``` bash
source $HOME/.cargo/env
```

Verify installation:

``` bash
rustc --version
cargo --version
```

------------------------------------------------------------------------

# 3️⃣ Create the Project

``` bash
cargo new kube-devops
cd kube-devops
```

Project structure:

    kube-devops/
     ├── Cargo.toml
     └── src/
         └── main.rs

------------------------------------------------------------------------

# 4️⃣ Understanding Cargo.toml

``` toml
[package]
name = "kube-devops"
version = "0.1.0"
edition = "2021"

[dependencies]
```

Comparable to:

-   package.json
-   go.mod
-   requirements.txt

------------------------------------------------------------------------

# 5️⃣ Your First Rust Program

Replace `src/main.rs`:

``` rust
fn main() {
    println!("Kube DevOps Tool");
}
```

Run:

``` bash
cargo run
```

Production build:

``` bash
cargo build --release
```

Binary location:

    target/release/kube-devops

------------------------------------------------------------------------

# 6️⃣ Introducing Modules

Recommended structure:

    src/
     ├── main.rs
     ├── cli.rs
     └── commands/
          ├── mod.rs
          ├── version.rs
          └── check.rs

Create structure:

``` bash
mkdir src/commands
touch src/cli.rs
touch src/commands/mod.rs
touch src/commands/version.rs
touch src/commands/check.rs
```

------------------------------------------------------------------------

# 7️⃣ Add CLI Dependency

In `Cargo.toml`:

``` toml
[dependencies]
clap = { version = "4", features = ["derive"] }
```

Then:

``` bash
cargo build
```

------------------------------------------------------------------------

# 8️⃣ Implement CLI Parsing

## cli.rs

``` rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kube-devops")]
#[command(about = "Kubernetes DevOps Enhancement Tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Version,
    Check,
}
```

------------------------------------------------------------------------

# 9️⃣ Wire CLI Into main.rs

``` rust
mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Version => commands::version::run(),
        Commands::Check => commands::check::run(),
    }
}
```

------------------------------------------------------------------------

# 🔟 Implement Commands

## commands/mod.rs

``` rust
pub mod version;
pub mod check;
```

## commands/version.rs

``` rust
pub fn run() {
    println!("kube-devops version 0.1.0");
}
```

## commands/check.rs

``` rust
pub fn run() {
    println!("Running DevOps checks...");
}
```

------------------------------------------------------------------------

# 1️⃣1️⃣ Test It

``` bash
cargo run -- version
cargo run -- check
```

Expected output:

    kube-devops version 0.1.0
    Running DevOps checks...

------------------------------------------------------------------------

# 1️⃣2️⃣ Add Proper Error Handling (Professional Style)

### version.rs

``` rust
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("kube-devops version 0.1.0");
    Ok(())
}
```

### check.rs

``` rust
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running DevOps checks...");
    Ok(())
}
```

### main.rs

``` rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Version => commands::version::run()?,
        Commands::Check => commands::check::run()?,
    }

    Ok(())
}
```

The `?` operator propagates errors cleanly and idiomatically.

------------------------------------------------------------------------

# 🧠 What You Have Learned

-   Ownership fundamentals\
-   Borrowing model\
-   Structs\
-   Enums\
-   Modules\
-   Pattern matching\
-   Cargo basics\
-   CLI parsing with clap\
-   Result and error propagation\
-   Clean project architecture

These foundations prepare you for:

-   Async Rust\
-   Kubernetes API integration\
-   Controllers and operators\
-   Admission webhooks\
-   Production-grade DevOps tooling

------------------------------------------------------------------------

**Next Step:** Async Rust + Kubernetes Client Integration
