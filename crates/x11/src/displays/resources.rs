//! Settings from the X resource database: the display scale from `Xft.dpi`.

const BASE_DPI: f64 = 96.0;

/// `Xft.dpi / 96` from the `RESOURCE_MANAGER` string, or 1 when it isn't set.
pub fn scale(resources: &str) -> f64 {
    resources
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim() == "Xft.dpi")
        .and_then(|(_, value)| value.trim().parse::<f64>().ok())
        .filter(|dpi| dpi.is_finite() && *dpi > 0.0)
        .map_or(1.0, |dpi| dpi / BASE_DPI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_follows_xft_dpi() {
        assert_eq!(scale("Xft.antialias:\t1\nXft.dpi:\t144\n"), 1.5);
        assert_eq!(scale("Xft.dpi: 96"), 1.0);
        assert_eq!(scale("Xcursor.size:\t24\n"), 1.0);
        assert_eq!(scale("Xft.dpi:\tnope"), 1.0);
        assert_eq!(scale(""), 1.0);
    }
}
