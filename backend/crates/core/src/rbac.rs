//! Rollenbasierte Zugriffssteuerung (RBAC).
//!
//! Drei Rollen gemäß Datenmodell (docs/architecture.md): `super_admin`
//! (systemweit), `domain_admin` (auf eine oder mehrere Domains beschränkt),
//! `user` (nur eigenes Postfach/eigene Einstellungen). Jede Berechtigungs-
//! prüfung muss serverseitig erfolgen — Client-Filterung ist nur UX, kein
//! Sicherheitsmechanismus (siehe Sicherheitsmodell).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    SuperAdmin,
    DomainAdmin,
    User,
}

/// Administrative Aktionen, deren Erlaubnis vom Kontext (eigene Domain,
/// eigener Account) abhängt. Wird von der REST-API (M2) pro Endpunkt genutzt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ManageDomain,
    ManageDomainUsers,
    ManageOwnAccount,
    ManageSystemSettings,
    ViewAuditLog,
}

/// Der Sicherheitskontext eines authentifizierten Requests: Rolle plus
/// optionaler Domain-Scope (aus dem JWT, siehe `auth::jwt::Claims`).
#[derive(Debug, Clone)]
pub struct Actor {
    pub user_id: Uuid,
    pub role: Role,
    /// Bei `DomainAdmin`/`User`: die Domain, auf die der Actor beschränkt ist.
    /// Bei `SuperAdmin`: `None` (kein Scope-Limit).
    pub domain_id: Option<Uuid>,
}

impl Actor {
    /// Prüft, ob dieser Actor `action` auf der Ressource `target_domain_id`
    /// ausführen darf. `target_domain_id = None` bedeutet eine systemweite
    /// Ressource (z. B. globale Einstellungen), die nur `SuperAdmin` betreffen darf.
    pub fn can(&self, action: Action, target_domain_id: Option<Uuid>) -> bool {
        match (self.role, action) {
            (Role::SuperAdmin, _) => true,

            (Role::DomainAdmin, Action::ManageDomain | Action::ManageDomainUsers) => {
                target_domain_id.is_some() && target_domain_id == self.domain_id
            }
            (Role::DomainAdmin, Action::ViewAuditLog) => {
                target_domain_id.is_some() && target_domain_id == self.domain_id
            }
            (Role::DomainAdmin, Action::ManageOwnAccount) => true,
            (Role::DomainAdmin, Action::ManageSystemSettings) => false,

            (Role::User, Action::ManageOwnAccount) => true,
            (Role::User, _) => false,
        }
    }

    /// Prüft zusätzlich, ob der Actor der Eigentümer eines Nutzer-scoped
    /// Objekts ist (z. B. eigenes App-Passwort, eigene Session) — für
    /// `Action::ManageOwnAccount` genügt Rollen-Prüfung nicht, die
    /// Identität muss übereinstimmen.
    pub fn owns(&self, resource_owner_id: Uuid) -> bool {
        self.user_id == resource_owner_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn super_admin_can_do_everything() {
        let actor = Actor {
            user_id: uuid(1),
            role: Role::SuperAdmin,
            domain_id: None,
        };
        assert!(actor.can(Action::ManageSystemSettings, None));
        assert!(actor.can(Action::ManageDomain, Some(uuid(2))));
    }

    #[test]
    fn domain_admin_limited_to_own_domain() {
        let own_domain = uuid(2);
        let other_domain = uuid(3);
        let actor = Actor {
            user_id: uuid(1),
            role: Role::DomainAdmin,
            domain_id: Some(own_domain),
        };

        assert!(actor.can(Action::ManageDomain, Some(own_domain)));
        assert!(!actor.can(Action::ManageDomain, Some(other_domain)));
        assert!(!actor.can(Action::ManageSystemSettings, None));
    }

    #[test]
    fn domain_admin_without_scope_is_denied_domain_actions() {
        // Verteidigung gegen fehlerhaft ausgestelltes Token ohne domain_id.
        let actor = Actor {
            user_id: uuid(1),
            role: Role::DomainAdmin,
            domain_id: None,
        };
        assert!(!actor.can(Action::ManageDomain, Some(uuid(2))));
    }

    #[test]
    fn plain_user_can_only_manage_own_account() {
        let actor = Actor {
            user_id: uuid(1),
            role: Role::User,
            domain_id: Some(uuid(2)),
        };
        assert!(actor.can(Action::ManageOwnAccount, None));
        assert!(!actor.can(Action::ManageDomain, Some(uuid(2))));
        assert!(!actor.can(Action::ViewAuditLog, Some(uuid(2))));
    }

    #[test]
    fn ownership_check_prevents_cross_user_access() {
        let actor = Actor {
            user_id: uuid(1),
            role: Role::User,
            domain_id: Some(uuid(2)),
        };
        assert!(actor.owns(uuid(1)));
        assert!(!actor.owns(uuid(9)));
    }
}
