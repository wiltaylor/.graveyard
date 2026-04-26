use anyhow::Result;
use rand::distributions::Alphanumeric;
use rand::rngs::OsRng;
use rand::Rng;
use sqlx::SqlitePool;

use crate::model::user;

const RANDOM_PASSWORD_LEN: usize = 16;

fn generate_password() -> String {
    let bytes: Vec<u8> = (0..RANDOM_PASSWORD_LEN)
        .map(|_| OsRng.sample(Alphanumeric))
        .collect();
    String::from_utf8(bytes).expect("alphanumeric is valid utf-8")
}

fn print_credentials(action: &str, username: &str, password: &str) {
    eprintln!("============================================================");
    eprintln!("aciddog-manager: {action}");
    eprintln!("  username: {username}");
    eprintln!("  password: {password}");
    eprintln!("Save this now — it will not be shown again.");
    eprintln!("============================================================");
}

/// On first start (no users in db), seed an `admin` user with a random
/// password and print the credentials to stderr. No-op once any user
/// exists — operators use `reset-password` from then on.
pub async fn ensure_admin_user(pool: &SqlitePool) -> Result<()> {
    if user::count(pool).await? > 0 {
        return Ok(());
    }
    let password = generate_password();
    user::insert(pool, "admin", &password).await?;
    print_credentials("seeded admin user", "admin", &password);
    Ok(())
}

/// Upsert a user's password. Creates the user if they don't exist;
/// updates the password if they do. If `password` is `None`, generates
/// a random 16-char password and prints it. Used by both the
/// `reset-password` subcommand and the `run-manager-remote-demo` recipe.
pub async fn reset_password_command(
    pool: &SqlitePool,
    username: &str,
    password: Option<&str>,
) -> Result<()> {
    let (resolved_password, generated) = match password {
        Some(p) => (p.to_string(), false),
        None => (generate_password(), true),
    };

    let existing = user::find_by_username(pool, username).await?;
    let action = if existing.is_some() {
        user::update_password(pool, username, &resolved_password).await?;
        "reset password"
    } else {
        user::insert(pool, username, &resolved_password).await?;
        "created user"
    };

    if generated {
        print_credentials(action, username, &resolved_password);
    } else {
        eprintln!("aciddog-manager: {action} for `{username}`");
    }
    Ok(())
}
