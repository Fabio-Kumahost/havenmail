//! Bootstrap des allerersten `super_admin`-Kontos während der Installation
//! (M5). Läuft lokal gegen die Datenbank, bevor überhaupt ein Login-Token
//! existiert — es gibt bewusst keinen API-Endpunkt dafür (kein
//! unauthentifizierter Netzwerkweg, um ein super_admin-Konto anzulegen).
//! Idempotent: ein erneuter install.sh-Lauf legt weder die Domain noch den
//! Admin doppelt an.

use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::password::{hash_password, PasswordError};

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("Datenbankfehler: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Passwort-Hashing fehlgeschlagen: {0}")]
    Password(#[from] PasswordError),
}

pub enum BootstrapOutcome {
    /// Domain und/oder Admin wurden neu angelegt.
    Created { domain_id: Uuid, user_id: Uuid },
    /// Ein super_admin für diese Domain existierte bereits — nichts verändert.
    AlreadyExists { domain_id: Uuid, user_id: Uuid },
}

/// Legt die angegebene Domain an (falls nicht vorhanden) und darin einen
/// `super_admin`-Benutzer mit dem übergebenen Klartextpasswort (falls noch
/// kein super_admin für diese Domain existiert).
pub async fn bootstrap_super_admin(
    pool: &PgPool,
    domain_name: &str,
    admin_local_part: &str,
    admin_password: &str,
) -> Result<BootstrapOutcome, BootstrapError> {
    let mut tx = pool.begin().await?;

    let domain_id: Uuid = sqlx::query_scalar(
        r#"
        insert into domains (name)
        values ($1)
        on conflict (name) do update set name = excluded.name
        returning id
        "#,
    )
    .bind(domain_name)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        "select id from users where domain_id = $1 and role = 'super_admin' limit 1",
    )
    .bind(domain_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        tx.commit().await?;
        return Ok(BootstrapOutcome::AlreadyExists {
            domain_id,
            user_id: existing_id,
        });
    }

    let password_hash = hash_password(admin_password)?;
    let user_id: Uuid = sqlx::query_scalar(
        r#"
        insert into users (domain_id, local_part, password_hash, role)
        values ($1, $2, $3, 'super_admin')
        on conflict (domain_id, local_part) do update set role = 'super_admin'
        returning id
        "#,
    )
    .bind(domain_id)
    .bind(admin_local_part)
    .bind(&password_hash)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(BootstrapOutcome::Created { domain_id, user_id })
}
