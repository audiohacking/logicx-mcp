//! MIDI dispatcher validation helpers (logic-pro-mcp MIDIDispatcherValidationHelpersTests).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

pub type ValidationResult<T> = Result<T, ValidationError>;

pub fn validate_port(params: &[(String, String)]) -> ValidationResult<&'static str> {
    let port = params
        .iter()
        .find(|(k, _)| k == "port")
        .map(|(_, v)| v.as_str());
    match port {
        None => Ok("midi"),
        Some("") => Err(ValidationError {
            message: "port must be 'midi' or 'keycmd'".into(),
        }),
        Some("midi") => Ok("midi"),
        Some("keycmd") => Ok("keycmd"),
        Some("scripter") => Err(ValidationError {
            message: "port 'scripter' is not supported for logic_midi — use logic_mixer.set_plugin_param"
                .into(),
        }),
        Some(other) => Err(ValidationError {
            message: format!("unknown port '{other}' — use 'midi' or 'keycmd'"),
        }),
    }
}

/// Map 1-based MIDI channel (1..=16) to wire byte (0..=15).
pub fn validate_midi_channel(params: &[(String, String)]) -> ValidationResult<u8> {
    let raw = params.iter().find(|(k, _)| k == "channel").map(|(_, v)| v.as_str());
    let Some(s) = raw else {
        return Ok(0);
    };
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false") {
        return Err(ValidationError {
            message: "channel must be integer 1..16".into(),
        });
    }
    let parsed = if let Ok(v) = s.parse::<i32>() {
        Some(v)
    } else if let Ok(f) = s.parse::<f64>() {
        if !f.is_finite() {
            return Err(ValidationError {
                message: "channel must be integer 1..16".into(),
            });
        }
        if (f - f.round()).abs() > f64::EPSILON {
            return Err(ValidationError {
                message: "channel must be integer 1..16".into(),
            });
        }
        Some(f.round() as i32)
    } else {
        None
    };
    let Some(v) = parsed else {
        return Err(ValidationError {
            message: "channel must be integer 1..16".into(),
        });
    };
    if (1..=16).contains(&v) {
        Ok((v - 1) as u8)
    } else {
        Err(ValidationError {
            message: "channel must be integer 1..16".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn validate_port_defaults_midi() {
        assert_eq!(validate_port(&[]).unwrap(), "midi");
    }

    #[test]
    fn validate_port_keycmd_and_midi() {
        assert_eq!(validate_port(&p(&[("port", "keycmd")])).unwrap(), "keycmd");
        assert_eq!(validate_port(&p(&[("port", "midi")])).unwrap(), "midi");
    }

    #[test]
    fn validate_port_rejects_scripter_and_unknown() {
        assert!(validate_port(&p(&[("port", "scripter")])).is_err());
        let err = validate_port(&p(&[("port", "foo")])).unwrap_err();
        assert!(err.message.contains("midi"));
        assert!(err.message.contains("keycmd"));
    }

    #[test]
    fn validate_port_rejects_empty() {
        assert!(validate_port(&p(&[("port", "")])).is_err());
    }

    #[test]
    fn validate_channel_wire_mapping() {
        assert_eq!(validate_midi_channel(&p(&[("channel", "1")])).unwrap(), 0);
        assert_eq!(validate_midi_channel(&p(&[("channel", "16")])).unwrap(), 15);
        assert_eq!(validate_midi_channel(&p(&[("channel", "5")])).unwrap(), 4);
    }

    #[test]
    fn validate_channel_missing_defaults_wire_0() {
        assert_eq!(validate_midi_channel(&[]).unwrap(), 0);
    }

    #[test]
    fn validate_channel_rejects_out_of_range() {
        assert!(validate_midi_channel(&p(&[("channel", "0")])).is_err());
        assert!(validate_midi_channel(&p(&[("channel", "17")])).is_err());
        assert!(validate_midi_channel(&p(&[("channel", "-1")])).is_err());
    }

    #[test]
    fn validate_channel_rejects_fractional_and_bool() {
        assert!(validate_midi_channel(&p(&[("channel", "1.5")])).is_err());
        assert!(validate_midi_channel(&p(&[("channel", "true")])).is_err());
        assert!(validate_midi_channel(&p(&[("channel", "inf")])).is_err());
    }
}
