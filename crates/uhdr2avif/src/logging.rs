
#[derive(Default)]
pub struct LoggingConfig {
    output_to_file: bool,
}

impl LoggingConfig {
    pub fn output_to_file(mut self) -> Self {
        self.output_to_file = true;
        self
    }

    pub fn apply(self) {
        use log::Level;

        use fern::colors::{Color, ColoredLevelConfig};

        let base_config = fern::Dispatch::new();

        let colors_line = ColoredLevelConfig::new()
          .error(Color::Red)
          .warn(Color::Yellow)
          .info(Color::White) // Default
          .debug(Color::BrightMagenta) // Default
          .trace(Color::BrightBlack);
        let colors_level = colors_line.info(Color::Green);

        let stderr_config = fern::Dispatch::new()
          .format(move |out, message, record| {
            out.finish(format_args!(
              "{begin_color_line}{date} {colored_level}{begin_color_line} [{target}]{file_line} {message}\x1B[0m",
              begin_color_line = format_args!(
                "\x1B[{}m",
                colors_line.get_color(&record.level()).to_fg_str()
              ),
              date = chrono::Local::now().format("[%Y/%m/%d %H:%M:%S]"),
              colored_level = colors_level.color(record.level()),
              target = record.target(),
              file_line = match record.level() {
                Level::Error | Level::Warn | Level::Debug => {
                  format!(
                    " [{file}:{line}]",
                    file = record.file().unwrap_or("N/A"),
                    line = record.line().unwrap_or(0)
                  )
                },
                _ => {
                  Default::default()
                }
              }
            ));
          })
          .chain(std::io::stderr());

        let mut config = base_config
          .chain(stderr_config);

        if self.output_to_file {
          let log_file = fern::log_file("log.txt")
            .unwrap();

          let log_file_config = fern::Dispatch::new()
            .format(|out, message, record| {
              out.finish(format_args!(
                "{date} {level} [{target}]{file_line} {message}",
                date = chrono::Local::now().format("[%Y/%m/%d %H:%M:%S]"),
                level = record.level(),
                target = record.target(),
                file_line = match record.level() {
                  Level::Error | Level::Warn | Level::Debug => {
                    format!(
                      " [{file}:{line}]",
                      file = record.file().unwrap_or("N/A"),
                      line = record.line().unwrap_or(0)
                    )
                  },
                  _ => {
                    Default::default()
                  }
                }
              ));
            })
            .chain(log_file);

            config = config
                .chain(log_file_config);
        }

        config
          .apply()
          .unwrap();
    }
}
