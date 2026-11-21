//! M3 control requests are validated before they enter the player worker.
//! The same worker that owns libmpv executes all control operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerControl {
    Pause(bool),
    SeekAbsolute(f64),
    SetRate(f64),
    SetVolume(f64),
}

impl PlayerControl {
    pub fn validate(self) -> Result<Self, &'static str> {
        match self {
            Self::Pause(_) => {}
            Self::SeekAbsolute(seconds) if seconds.is_finite() && seconds >= 0.0 => {}
            Self::SetRate(rate) if rate.is_finite() && (0.5..=2.0).contains(&rate) => {}
            Self::SetVolume(volume) if volume.is_finite() && (0.0..=100.0).contains(&volume) => {}
            Self::SeekAbsolute(_) => return Err("Seek position must be finite and non-negative"),
            Self::SetRate(_) => return Err("Playback rate must be between 0.5 and 2.0"),
            Self::SetVolume(_) => return Err("Volume must be between 0 and 100"),
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::PlayerControl as C;

    #[test]
    fn rejects_nonfinite_and_out_of_bounds_user_input() {
        assert!(C::SeekAbsolute(0.0).validate().is_ok());
        assert!(C::SeekAbsolute(42.5).validate().is_ok());
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert!(C::SeekAbsolute(value).validate().is_err());
            assert!(C::SetRate(value).validate().is_err());
            assert!(C::SetVolume(value).validate().is_err());
        }
        for rate in [0.5, 1.0, 1.5, 2.0] {
            assert!(C::SetRate(rate).validate().is_ok());
        }
        assert!(C::SetRate(0.49).validate().is_err());
        assert!(C::SetRate(2.01).validate().is_err());
        assert!(C::SetVolume(0.0).validate().is_ok());
        assert!(C::SetVolume(100.0).validate().is_ok());
        assert!(C::SetVolume(101.0).validate().is_err());
        assert!(C::Pause(true).validate().is_ok());
        assert!(C::Pause(false).validate().is_ok());
    }
}
