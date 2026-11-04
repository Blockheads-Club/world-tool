//! The world record (`main/worldv2`): name, id, mode, size, spawn, clocks. See `docs/client-save-format.md` §5.

use plist::{Dictionary, Value};

use crate::error::{Error, Result};
use crate::stored::{self, Encoding};

/// Length of a game day in `worldTime` seconds. Assumed (24 minutes of play), not yet confirmed in the game.
pub const DAY_SECONDS: f64 = 1440.0;

/// The parsed world record.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldRecord {
    /// The record's dictionary.
    pub dict: Dictionary,
    enc: Encoding,
}

impl WorldRecord {
    /// Parses a stored record.
    pub fn decode(bytes: &[u8]) -> Result<WorldRecord> {
        let (v, enc) = stored::decode(bytes)?;
        match v {
            Value::Dictionary(dict) => Ok(WorldRecord { dict, enc }),
            _ => Err(Error::Format("world record is not a dictionary".into())),
        }
    }

    /// Encodes the record the way it was stored.
    pub fn encode(&self) -> Result<Vec<u8>> {
        stored::encode(&Value::Dictionary(self.dict.clone()), self.enc)
    }

    fn int(&self, k: &str) -> Option<i64> {
        self.dict.get(k).and_then(Value::as_signed_integer)
    }

    /// `worldName`.
    pub fn name(&self) -> Option<&str> {
        self.dict.get("worldName").and_then(Value::as_string)
    }
    /// `saveID`.
    pub fn save_id(&self) -> Option<&str> {
        self.dict.get("saveID").and_then(Value::as_string)
    }
    /// `expertMode`.
    pub fn expert(&self) -> bool {
        self.dict.get("expertMode").and_then(Value::as_boolean).unwrap_or(false)
    }
    /// True for a world with custom rules.
    pub fn custom_rules(&self) -> bool {
        self.dict.contains_key("customRulesDict")
    }
    /// `worldWidthMacro` (world width in macro blocks).
    pub fn width_macro(&self) -> Option<u32> {
        self.int("worldWidthMacro").and_then(|w| u32::try_from(w).ok())
    }
    /// `randomSeed`.
    pub fn seed(&self) -> Option<i64> {
        self.int("randomSeed")
    }
    /// `saveVersion`.
    pub fn save_version(&self) -> Option<i64> {
        self.int("saveVersion")
    }
    /// Spawn portal tile (`startPortalPos.x/y`).
    pub fn spawn(&self) -> Option<(i64, i64)> {
        Some((self.int("startPortalPos.x")?, self.int("startPortalPos.y")?))
    }
    /// `worldTime` in seconds.
    pub fn world_time(&self) -> Option<f64> {
        self.dict.get("worldTime").and_then(Value::as_real)
    }
    /// `creationDate` as Unix seconds.
    pub fn created(&self) -> Option<i64> {
        self.date("creationDate")
    }
    /// `saveDate` as Unix seconds.
    pub fn saved(&self) -> Option<i64> {
        self.date("saveDate")
    }
    /// True for a client's cache of a multiplayer world (`remoteGame`).
    pub fn remote(&self) -> bool {
        self.dict.get("remoteGame").and_then(Value::as_boolean).unwrap_or(false)
    }

    fn date(&self, k: &str) -> Option<i64> {
        let d: std::time::SystemTime = self.dict.get(k)?.as_date()?.into();
        Some(match d.duration_since(std::time::UNIX_EPOCH) {
            Ok(x) => x.as_secs() as i64,
            Err(e) => -(e.duration().as_secs() as i64),
        })
    }

    /// Renames the world (the game stores names in upper case; so does this).
    pub fn set_name(&mut self, name: &str) -> Result<()> {
        let n = name.trim();
        if n.is_empty() || n.chars().count() > 64 {
            return Err(Error::Invalid("a world name needs 1 to 64 characters".into()));
        }
        self.dict.insert("worldName".into(), Value::String(n.to_uppercase()));
        Ok(())
    }

    /// Sets the time of day (hour 0..24) keeping the day count.
    pub fn set_time_of_day(&mut self, hour: f64) -> Result<()> {
        if !(0.0..24.0).contains(&hour) {
            return Err(Error::Invalid("hour must be in 0..24".into()));
        }
        let t = self.world_time().unwrap_or(0.0);
        let day = (t / DAY_SECONDS).floor();
        self.dict.insert("worldTime".into(), Value::Real(day * DAY_SECONDS + hour / 24.0 * DAY_SECONDS));
        Ok(())
    }

    /// Time of day (hour 0..24).
    pub fn time_of_day(&self) -> Option<f64> {
        let t = self.world_time()?;
        Some(t.rem_euclid(DAY_SECONDS) / DAY_SECONDS * 24.0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn record(name: &str, width: i64) -> Vec<u8> {
        let mut d = Dictionary::new();
        d.insert("worldName".into(), Value::String(name.into()));
        d.insert("saveID".into(), Value::String("0123456789abcdef0123456789abcdef".into()));
        d.insert("worldWidthMacro".into(), Value::Integer(width.into()));
        d.insert("randomSeed".into(), Value::Integer(42.into()));
        d.insert("saveVersion".into(), Value::Integer(1100.into()));
        d.insert("startPortalPos.x".into(), Value::Integer(40.into()));
        d.insert("startPortalPos.y".into(), Value::Integer(520.into()));
        d.insert("worldTime".into(), Value::Real(3000.0));
        d.insert("expertMode".into(), Value::Boolean(true));
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        d.insert("creationDate".into(), Value::Date(t.into()));
        let before = std::time::UNIX_EPOCH - std::time::Duration::from_secs(100);
        d.insert("saveDate".into(), Value::Date(before.into()));
        stored::encode(&Value::Dictionary(d), Encoding::CLIENT).unwrap()
    }

    #[test]
    fn fields() {
        let mut r = WorldRecord::decode(&record("SUNNY", 4)).unwrap();
        assert_eq!(r.name(), Some("SUNNY"));
        assert_eq!(r.save_id().unwrap().len(), 32);
        assert!(r.expert() && !r.custom_rules() && !r.remote());
        assert_eq!((r.width_macro(), r.seed(), r.save_version()), (Some(4), Some(42), Some(1100)));
        assert_eq!(r.spawn(), Some((40, 520)));
        assert_eq!(r.created(), Some(1_700_000_000));
        assert_eq!(r.saved(), Some(-100));
        assert!((r.time_of_day().unwrap() - 2.0).abs() < 1e-9);
        r.set_time_of_day(12.0).unwrap();
        assert_eq!(r.world_time(), Some(3600.0));
        assert!(r.set_time_of_day(24.0).is_err());
        r.set_name("  pirate cove ").unwrap();
        assert_eq!(r.name(), Some("PIRATE COVE"));
        assert!(r.set_name(" ").is_err());
        assert!(r.set_name(&"x".repeat(65)).is_err());
        let back = WorldRecord::decode(&r.encode().unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(WorldRecord::decode(&stored::encode(&Value::Integer(1.into()), Encoding::CLIENT).unwrap()).is_err());
        let mut empty = WorldRecord { dict: Dictionary::new(), enc: Encoding::CLIENT };
        assert_eq!((empty.name(), empty.width_macro(), empty.spawn(), empty.created()), (None, None, None, None));
        assert_eq!(empty.time_of_day(), None);
        empty.set_time_of_day(6.0).unwrap();
        assert_eq!(empty.world_time(), Some(360.0));
    }
}
