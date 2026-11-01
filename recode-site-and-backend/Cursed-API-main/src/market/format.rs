//! Which game versions a resource pack is for: pack.mcmeta read the way 1.21.11 reads it.
//!
//! A port of net.minecraft.server.packs.metadata.pack.PackFormat (validate, validateNewFormat, validateOldFormat) for
//! the resource-pack side: the same fields, the same errors, the same range. The game then calls a pack compatible
//! when 75.0 (its own format, SharedConstants.RESOURCE_PACK_FORMAT_MAJOR/MINOR) falls inside that range; a pack whose
//! pack.mcmeta fails here still loads, but the game marks it as made for another version.

use serde_json::Value;

/// A pack format: major.minor. A maximum given as a bare number takes every minor of its major.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Format(pub i64, pub i64);

pub const GAME: Format = Format(75, 0); // 1.21.11
pub const GAME_NAME: &str = "1.21.11";
const LAST_PRE_MINOR: i64 = 64; // PackFormat.lastPreMinorVersion(CLIENT_RESOURCES)
const ANY_MINOR: i64 = i32::MAX as i64;

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.1 == ANY_MINOR { write!(f, "{}.*", self.0) } else { write!(f, "{}.{}", self.0, self.1) }
    }
}

/// A format as a bare number or a list of one or two numbers (ExtraCodecs.compactListCodec of non-negative ints).
fn format_field(v: &Value, missing_minor: i64) -> Result<Format, String> {
    let nonneg = |x: &Value| x.as_i64().filter(|n| *n >= 0 && *n <= i32::MAX as i64).ok_or_else(|| "a format must be a non-negative whole number".to_string());
    match v {
        Value::Array(list) if !list.is_empty() && list.len() <= 256 => {
            let major = nonneg(&list[0])?;
            let minor = if list.len() > 1 { nonneg(&list[1])? } else { missing_minor };
            Ok(Format(major, minor))
        }
        Value::Array(_) => Err("a format list must have one to 256 numbers".into()),
        _ => Ok(Format(nonneg(v)?, missing_minor)),
    }
}

/// supported_formats: a number, [min, max] or {"min_inclusive": …, "max_inclusive": …} (InclusiveRange.codec).
fn range_field(v: &Value) -> Result<(i64, i64), String> {
    let int = |x: &Value| x.as_i64().filter(|n| n.abs() <= i32::MAX as i64).ok_or_else(|| "supported_formats must hold whole numbers".to_string());
    let (a, b) = match v {
        Value::Array(list) if list.len() == 2 => (int(&list[0])?, int(&list[1])?),
        Value::Object(o) => (
            int(o.get("min_inclusive").ok_or("supported_formats is missing min_inclusive")?)?,
            int(o.get("max_inclusive").ok_or("supported_formats is missing max_inclusive")?)?,
        ),
        Value::Array(_) => return Err("supported_formats must be two numbers".into()),
        _ => {
            let n = int(v)?;
            (n, n)
        }
    };
    if a > b {
        return Err(format!("supported_formats goes from {a} down to {b}"));
    }
    Ok((a, b))
}

/// The range of formats the pack declares, or the game's own complaint about it.
pub fn declared(pack: &Value) -> Result<(Format, Format), String> {
    let min = pack.get("min_format").map(|v| format_field(v, 0)).transpose()?;
    let max = pack.get("max_format").map(|v| format_field(v, ANY_MINOR)).transpose()?;
    let format = pack.get("pack_format").map(|v| v.as_i64().filter(|n| n.abs() <= i32::MAX as i64).ok_or("pack_format must be a whole number")).transpose()?;
    let supported = pack.get("supported_formats").map(range_field).transpose()?;

    let for_range = |i: i64, j: i64| -> Result<(), String> {
        let f = format.expect("checked by the caller");
        if f < i || f > j {
            Err(format!("Pack declared support for versions {i} to {j} but declared main format is {f}"))
        } else if f < 15 {
            Err("Multi-version packs cannot support minimum version of less than 15, since this will leave versions in range unable to load pack.".into())
        } else {
            Ok(())
        }
    };
    let need_format = |i: i64| -> Result<(), String> {
        if format.is_none() {
            Err(format!(
                "Pack declares support for formats up to {LAST_PRE_MINOR}, but game versions supporting formats 17 to {LAST_PRE_MINOR} require a pack_format field. Add \"pack_format\": {i} or require a version greater or equal to {}.0.",
                LAST_PRE_MINOR + 1
            ))
        } else {
            Ok(())
        }
    };

    if min.is_some() != max.is_some() {
        return Err("Pack missing field, must declare both min_format and max_format".into());
    }
    if let (Some(min), Some(max)) = (min, max) {
        let (i, j) = (min.0, max.0);
        if min > max {
            return Err(format!("Pack min_format ({min}) is greater than max_format ({max})"));
        }
        if i > LAST_PRE_MINOR {
            if supported.is_some() {
                return Err(format!("Pack key supported_formats is deprecated starting from pack format {}. Remove supported_formats from your pack.mcmeta.", LAST_PRE_MINOR + 1));
            }
            if format.is_some() {
                for_range(i, j)?;
            }
        } else {
            let Some((si, sj)) = supported else {
                return Err(format!(
                    "Pack declares support for format {i}, but game versions supporting formats 17 to {LAST_PRE_MINOR} require a supported_formats field. Add \"supported_formats\": [{i}, {LAST_PRE_MINOR}] or require a version greater or equal to {}.0.",
                    LAST_PRE_MINOR + 1
                ));
            };
            if si != i {
                return Err(format!("Pack version declaration mismatch between supported_formats (from {si}) and min_format ({min})"));
            }
            if sj != j && sj != LAST_PRE_MINOR {
                return Err(format!("Pack version declaration mismatch between supported_formats (up to {sj}) and max_format ({max})"));
            }
            need_format(i)?;
            for_range(i, j)?;
        }
        return Ok((min, max));
    }
    if let Some((i, j)) = supported {
        if j > LAST_PRE_MINOR {
            return Err(format!("Pack declares support for version newer than {LAST_PRE_MINOR}, but is missing mandatory fields min_format and max_format"));
        }
        need_format(i)?;
        for_range(i, j)?;
        return Ok((Format(i, 0), Format(j, 0)));
    }
    if let Some(f) = format {
        if f > LAST_PRE_MINOR {
            return Err(format!("Pack declares support for version newer than {LAST_PRE_MINOR}, but is missing mandatory fields min_format and max_format"));
        }
        return Ok((Format(f, 0), Format(f, 0)));
    }
    Err("Pack could not be parsed, missing format version information".into())
}

/// Game versions by resource pack format (the formats of releases; snapshots in between have their own).
const RELEASES: &[(i64, &str, &str)] = &[
    (1, "1.6.1", "1.8.9"),
    (2, "1.9", "1.10.2"),
    (3, "1.11", "1.12.2"),
    (4, "1.13", "1.14.4"),
    (5, "1.15", "1.16.1"),
    (6, "1.16.2", "1.16.5"),
    (7, "1.17", "1.17.1"),
    (8, "1.18", "1.18.2"),
    (9, "1.19", "1.19.2"),
    (12, "1.19.3", "1.19.3"),
    (13, "1.19.4", "1.19.4"),
    (15, "1.20", "1.20.1"),
    (18, "1.20.2", "1.20.2"),
    (22, "1.20.3", "1.20.4"),
    (32, "1.20.5", "1.20.6"),
    (34, "1.21", "1.21.1"),
    (42, "1.21.2", "1.21.3"),
    (46, "1.21.4", "1.21.4"),
    (55, "1.21.5", "1.21.5"),
    (63, "1.21.6", "1.21.6"),
    (64, "1.21.7", "1.21.8"),
    (75, "1.21.11", "1.21.11"),
];

/// The game versions a range of formats covers, in words: "1.16.2–1.16.5", "1.21–1.21.4", "1.21.11 and newer".
pub fn versions(min: Format, max: Format) -> String {
    if min.0 > GAME.0 {
        return format!("a version newer than {GAME_NAME} (format {})", min.0);
    }
    let first = RELEASES.iter().find(|r| r.0 >= min.0);
    let last = RELEASES.iter().rev().find(|r| r.0 <= max.0);
    let open = max.0 > RELEASES.last().map_or(0, |r| r.0);
    match (first, last) {
        (Some(a), Some(b)) if a.0 <= b.0 => {
            let span = if a.1 == b.2 { a.1.to_string() } else { format!("{}–{}", a.1, b.2) };
            if open { format!("{} and newer", a.1) } else { span }
        }
        (Some(a), _) if open => format!("{} and newer", a.1),
        _ if min.0 == max.0 => format!("format {}", min.0),
        _ => format!("formats {}–{}", min.0, max.0),
    }
}

/// Whether 1.21.11 takes the pack as its own (PackCompatibility: inside the range).
pub fn fits(min: Format, max: Format) -> bool {
    min <= GAME && GAME <= max
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn range(v: serde_json::Value) -> Result<(Format, Format), String> {
        declared(&v)
    }

    #[test]
    fn an_old_single_format() {
        let (a, b) = range(json!({"pack_format": 6, "description": "x"})).unwrap();
        assert_eq!((a, b), (Format(6, 0), Format(6, 0)));
        assert_eq!(versions(a, b), "1.16.2–1.16.5");
        assert!(!fits(a, b));
    }

    #[test]
    fn supported_formats_in_every_shape() {
        for v in [json!([34, 46]), json!({"min_inclusive": 34, "max_inclusive": 46})] {
            let (a, b) = range(json!({"pack_format": 34, "supported_formats": v})).unwrap();
            assert_eq!(versions(a, b), "1.21–1.21.4");
        }
        assert!(range(json!({"supported_formats": [34, 46]})).unwrap_err().contains("require a pack_format"));
        assert!(range(json!({"pack_format": 50, "supported_formats": [34, 46]})).unwrap_err().contains("main format is 50"));
    }

    #[test]
    fn the_new_min_and_max() {
        let (a, b) = range(json!({"min_format": 75, "max_format": 75})).unwrap();
        assert_eq!((a, b), (Format(75, 0), Format(75, ANY_MINOR)));
        assert!(fits(a, b));
        assert_eq!(versions(a, b), "1.21.11");
        let (a, b) = range(json!({"min_format": [69, 0], "max_format": 1000})).unwrap();
        assert!(fits(a, b));
        assert!(versions(a, b).ends_with("and newer"));
        assert!(range(json!({"min_format": 75})).unwrap_err().contains("both min_format and max_format"));
        assert!(range(json!({"min_format": 80, "max_format": 75})).unwrap_err().contains("greater than"));
        assert!(range(json!({"min_format": 70, "max_format": 80, "supported_formats": [70, 80]})).unwrap_err().contains("deprecated"));
    }

    #[test]
    fn spanning_the_old_and_the_new() {
        // From 1.21.4 up to 1.21.11: the old fields for the old games, the new ones for the new.
        let ok = json!({"pack_format": 46, "supported_formats": [46, 64], "min_format": 46, "max_format": 75});
        let (a, b) = range(ok).unwrap();
        assert!(fits(a, b));
        assert_eq!(versions(a, b), "1.21.4–1.21.11");
        assert!(range(json!({"min_format": 46, "max_format": 75})).unwrap_err().contains("require a supported_formats"));
    }

    #[test]
    fn nothing_to_go_by() {
        assert!(range(json!({"description": "hi"})).unwrap_err().contains("missing format version"));
        assert!(range(json!({"pack_format": 99})).unwrap_err().contains("newer than 64"));
        assert_eq!(versions(Format(80, 0), Format(84, ANY_MINOR)), "a version newer than 1.21.11 (format 80)");
    }
}
