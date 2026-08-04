//! Einfacher In-Process-Rate-Limiter für den Login-Endpunkt (Brute-Force-
//! Schutz, siehe docs/architecture.md, Bedrohungsanalyse).
//!
//! Bewusst kein verteilter Zustand (Redis o. Ä.) — für ein Single-Node-
//! Deployment (siehe MVP-Abgrenzung) genügt In-Memory-Tracking pro
//! Prozess; bei horizontaler Skalierung müsste dies durch einen geteilten
//! Store ersetzt werden (dokumentierte Skalierungsgrenze).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_ATTEMPTS: usize = 5;
const WINDOW: Duration = Duration::from_secs(15 * 60);

pub struct LoginRateLimiter {
    attempts: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }
}

impl LoginRateLimiter {
    /// Gibt `true` zurück, wenn `ip` das Limit fehlgeschlagener Versuche
    /// innerhalb des Zeitfensters bereits erreicht hat (Request sollte mit
    /// 429 abgelehnt werden, ohne das Passwort überhaupt zu prüfen).
    pub fn is_blocked(&self, ip: IpAddr) -> bool {
        let mut attempts = self.attempts.lock().expect("Mutex poisoned");
        let now = Instant::now();
        let entry = attempts.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < WINDOW);
        entry.len() >= MAX_ATTEMPTS
    }

    /// Vermerkt einen fehlgeschlagenen Login-Versuch von `ip`.
    pub fn record_failure(&self, ip: IpAddr) {
        let mut attempts = self.attempts.lock().expect("Mutex poisoned");
        let now = Instant::now();
        let entry = attempts.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < WINDOW);
        entry.push(now);
    }

    /// Setzt den Zähler nach erfolgreichem Login zurück.
    pub fn record_success(&self, ip: IpAddr) {
        self.attempts.lock().expect("Mutex poisoned").remove(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip() -> IpAddr {
        "203.0.113.7".parse().unwrap()
    }

    #[test]
    fn allows_up_to_the_configured_limit() {
        let limiter = LoginRateLimiter::default();
        for _ in 0..MAX_ATTEMPTS - 1 {
            assert!(!limiter.is_blocked(ip()));
            limiter.record_failure(ip());
        }
        assert!(!limiter.is_blocked(ip()));
    }

    #[test]
    fn blocks_after_limit_reached() {
        let limiter = LoginRateLimiter::default();
        for _ in 0..MAX_ATTEMPTS {
            limiter.record_failure(ip());
        }
        assert!(limiter.is_blocked(ip()));
    }

    #[test]
    fn success_resets_the_counter() {
        let limiter = LoginRateLimiter::default();
        for _ in 0..MAX_ATTEMPTS {
            limiter.record_failure(ip());
        }
        assert!(limiter.is_blocked(ip()));
        limiter.record_success(ip());
        assert!(!limiter.is_blocked(ip()));
    }

    #[test]
    fn different_ips_are_tracked_independently() {
        let limiter = LoginRateLimiter::default();
        let other_ip: IpAddr = "198.51.100.4".parse().unwrap();
        for _ in 0..MAX_ATTEMPTS {
            limiter.record_failure(ip());
        }
        assert!(limiter.is_blocked(ip()));
        assert!(!limiter.is_blocked(other_ip));
    }
}
