extern crate clap;

use atty::Stream;
use clap::Arg;
use std::path::Path;

use std::sync::Arc;

use crate::commands::engine::*;
use crate::commands::utils::*;
use crate::commands::Command;
use crate::era::repair::{repair, EraRepairOptions};
use crate::report::*;

pub struct EraRepairCommand;

impl EraRepairCommand {
    fn cli<'a>(&self) -> clap::Command<'a> {
        let cmd = clap::Command::new(self.name())
            .color(clap::ColorChoice::Never)
            .version(crate::version::tools_version())
            .about("Repair binary era metadata, and write it to a different device or file")
            .arg(
                Arg::new("QUIET")
                    .help("Suppress output messages, return only exit code.")
                    .short('q')
                    .long("quiet"),
            )
            // options
            .arg(
                Arg::new("INPUT")
                    .help("Specify the input device")
                    .short('i')
                    .long("input")
                    .value_name("FILE")
                    .required(true),
            )
            .arg(
                Arg::new("OUTPUT")
                    .help("Specify the output device")
                    .short('o')
                    .long("output")
                    .value_name("FILE")
                    .required(true),
            );
        engine_args(cmd)
    }
}

impl<'a> Command<'a> for EraRepairCommand {
    fn name(&self) -> &'a str {
        "era_repair"
    }

    fn run(&self, args: &mut dyn Iterator<Item = std::ffi::OsString>) -> exitcode::ExitCode {
        let matches = self.cli().get_matches_from(args);

        let input_file = Path::new(matches.value_of("INPUT").unwrap());
        let output_file = Path::new(matches.value_of("OUTPUT").unwrap());

        let report = if matches.is_present("QUIET") {
            std::sync::Arc::new(mk_quiet_report())
        } else if atty::is(Stream::Stdout) {
            std::sync::Arc::new(mk_progress_bar_report())
        } else {
            Arc::new(mk_simple_report())
        };

        check_input_file(input_file, &report);

        let engine_opts = parse_engine_opts(ToolType::Era, true, &matches);
        if engine_opts.is_err() {
            return to_exit_code(&report, engine_opts);
        }

        let opts = EraRepairOptions {
            input: input_file,
            output: output_file,
            engine_opts: engine_opts.unwrap(),
            report: report.clone(),
        };

        to_exit_code(&report, repair(opts))
    }
}
