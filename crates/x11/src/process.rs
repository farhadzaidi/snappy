//! Facts about client processes from `/proc`, for identity and app names.

/// The process start time in clock ticks since boot: field 22 of `/proc/<pid>/stat`.
pub fn start_time(pid: u32) -> Option<f64> {
    parse_start_time(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// The executable name, `/proc/<pid>/comm`.
pub fn executable(pid: u32) -> Option<String> {
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// The command name in field 2 may hold spaces and parentheses, so fields are counted
/// from its closing parenthesis; field 22 is the 20th after it.
pub fn parse_start_time(stat: &str) -> Option<f64> {
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_whitespace()
        .nth(19)?
        .parse::<u64>()
        .ok()
        .map(|ticks| ticks as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_time_is_field_twenty_two() {
        let stat = "4242 (my (odd) app) S 1 4242 4242 0 -1 4194560 1234 0 0 0 10 5 0 0 20 0 3 0 \
                    987654 123456789 2000 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 3 0 0";
        assert_eq!(parse_start_time(stat), Some(987_654.0));
    }

    #[test]
    fn malformed_stat_lines_have_no_start_time() {
        assert_eq!(parse_start_time("4242 (app) S 1 2"), None);
        assert_eq!(parse_start_time("garbage"), None);
    }

    #[test]
    fn this_process_has_a_start_time_and_name() {
        assert!(start_time(std::process::id()).is_some());
        assert!(executable(std::process::id()).is_some());
    }
}
