use super::*;

pub fn duration_from_str(text: &str) -> anyhow::Result<Duration> {
    fn parse_digits<T>(digits: &str, unit: &str) -> anyhow::Result<T>
    where
        T: FromStr,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        digits
            .parse()
            .with_context(|| format!("invalid number of {unit}: {digits}"))
    }

    if let Some(digits) = text.strip_suffix("ns") {
        return Ok(Duration::from_nanos(parse_digits(digits, "nanoseconds")?));
    }

    if let Some(digits) = text.strip_suffix("us") {
        return Ok(Duration::from_micros(parse_digits(digits, "microseconds")?));
    }

    if let Some(digits) = text.strip_suffix("ms") {
        return Ok(Duration::from_millis(parse_digits(digits, "milliseconds")?));
    }

    if let Some(digits) = text.strip_suffix("s") {
        return Ok(Duration::from_secs_f64(parse_digits(digits, "seconds")?));
    }

    if let Some(digits) = text.strip_suffix("m") {
        return Ok(Duration::from_secs_f64(
            parse_digits::<f64>(digits, "minutes")? * 60.0,
        ));
    }

    if let Some(digits) = text.strip_suffix("h") {
        return Ok(Duration::from_secs_f64(
            parse_digits::<f64>(digits, "minutes")? * 60.0 * 60.0,
        ));
    }

    if let Some(digits) = text.strip_suffix("d") {
        return Ok(Duration::from_secs_f64(
            parse_digits::<f64>(digits, "minutes")? * 60.0 * 60.0 * 24.0,
        ));
    }

    Err(anyhow!("not a valid duration specifier: {}", text))
}
