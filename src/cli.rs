//! Command-line handling for the binary: where the server listens.

pub const DEFAULT_BIND: &str = "127.0.0.1:8080";

#[derive(Debug, PartialEq)]
pub enum CliError {
    MissingBindValue,
    UnexpectedArgument(String),
}

/// Resolves the listen address from the arguments after the program name and the
/// value of the `BIND` environment variable: `--bind ADDR` or `--bind=ADDR` wins,
/// else `BIND`, else [`DEFAULT_BIND`]. When `--bind` is given more than once, the
/// last one wins. The address is returned unparsed; binding it reports bad values.
pub fn address_from(args: &[String], env_bind: Option<&str>) -> Result<String, CliError> {
    let mut flag = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--bind" {
            flag = Some(rest.next().ok_or(CliError::MissingBindValue)?.clone());
        } else if let Some(value) = arg.strip_prefix("--bind=") {
            flag = Some(value.to_string());
        } else {
            return Err(CliError::UnexpectedArgument(arg.clone()));
        }
    }
    Ok(flag
        .or_else(|| env_bind.map(str::to_string))
        .unwrap_or_else(|| DEFAULT_BIND.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_to_localhost_8080_with_no_flag_and_no_env() {
        assert_eq!(
            address_from(&args(&[]), None),
            Ok("127.0.0.1:8080".to_string())
        );
    }

    #[test]
    fn takes_the_bind_flag_with_a_separate_value() {
        assert_eq!(
            address_from(&args(&["--bind", "0.0.0.0:9000"]), None),
            Ok("0.0.0.0:9000".to_string())
        );
    }

    #[test]
    fn takes_the_bind_flag_with_an_equals_value() {
        assert_eq!(
            address_from(&args(&["--bind=0.0.0.0:9000"]), None),
            Ok("0.0.0.0:9000".to_string())
        );
    }

    #[test]
    fn takes_bind_from_the_environment_when_no_flag_is_given() {
        assert_eq!(
            address_from(&args(&[]), Some("10.0.0.1:7000")),
            Ok("10.0.0.1:7000".to_string())
        );
    }

    #[test]
    fn the_flag_wins_over_the_environment() {
        assert_eq!(
            address_from(&args(&["--bind", "0.0.0.0:9000"]), Some("10.0.0.1:7000")),
            Ok("0.0.0.0:9000".to_string())
        );
    }

    #[test]
    fn a_bind_flag_without_a_value_is_an_error() {
        assert_eq!(
            address_from(&args(&["--bind"]), None),
            Err(CliError::MissingBindValue)
        );
    }

    #[test]
    fn an_unknown_argument_is_an_error() {
        assert_eq!(
            address_from(&args(&["--port", "9000"]), None),
            Err(CliError::UnexpectedArgument("--port".to_string()))
        );
    }
}
