pub(crate) fn convert_filter(filter: log::LevelFilter) -> tracing_subscriber::filter::LevelFilter {
    match filter {
        log::LevelFilter::Off => tracing_subscriber::filter::LevelFilter::OFF,
        log::LevelFilter::Error => tracing_subscriber::filter::LevelFilter::ERROR,
        log::LevelFilter::Warn => tracing_subscriber::filter::LevelFilter::WARN,
        log::LevelFilter::Info => tracing_subscriber::filter::LevelFilter::INFO,
        log::LevelFilter::Debug => tracing_subscriber::filter::LevelFilter::DEBUG,
        log::LevelFilter::Trace => tracing_subscriber::filter::LevelFilter::TRACE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_conversion_info() {
        assert_eq!(
            convert_filter(log::LevelFilter::Info),
            tracing_subscriber::filter::LevelFilter::INFO
        );
    }

    #[test]
    fn test_filter_conversion_debug() {
        assert_eq!(
            convert_filter(log::LevelFilter::Debug),
            tracing_subscriber::filter::LevelFilter::DEBUG
        );
    }
}
