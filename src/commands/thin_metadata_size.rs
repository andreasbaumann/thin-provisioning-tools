extern crate clap;

use clap::Arg;
use std::ffi::OsString;

use crate::thin::metadata_size::{metadata_size, ThinMetadataSizeOptions};
use crate::units::*;

//------------------------------------------
use crate::commands::Command;

pub struct ThinMetadataSizeCommand;

impl ThinMetadataSizeCommand {
    fn cli<'a>(&self) -> clap::Command<'a> {
        clap::Command::new(self.name())
            .color(clap::ColorChoice::Never)
            .version(crate::version::tools_version())
            .about("Estimate the size of the metadata device needed for a given configuration.")
            // options
            .arg(
                Arg::new("BLOCK_SIZE")
                    .help("Specify the data block size")
                    .short('b')
                    .long("block-size")
                    .required(true)
                    .value_name("SECTORS"),
            )
            .arg(
                Arg::new("POOL_SIZE")
                    .help("Specify the size of pool device")
                    .short('s')
                    .long("pool-size")
                    .required(true)
                    .value_name("SECTORS"),
            )
            .arg(
                Arg::new("MAX_THINS")
                    .help("Maximum number of thin devices and snapshots")
                    .short('m')
                    .long("max-thins")
                    .required(true)
                    .value_name("NUM"),
            )
            .arg(
                Arg::new("UNIT")
                    .help("Specify the output unit")
                    .short('u')
                    .long("unit")
                    .value_name("UNIT")
                    .default_value("sector"),
            )
            .arg(
                Arg::new("NUMERIC_ONLY")
                    .help("Output numeric value only")
                    .short('n')
                    .long("numeric-only"),
            )
    }

    fn parse_args<I, T>(&self, args: I) -> (ThinMetadataSizeOptions, Units, bool)
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let matches = self.cli().get_matches_from(args);

        // TODO: handle unit suffix
        let pool_size = matches.value_of_t_or_exit::<u64>("POOL_SIZE");
        let block_size = matches.value_of_t_or_exit::<u32>("BLOCK_SIZE");
        let max_thins = matches.value_of_t_or_exit::<u64>("MAX_THINS");
        let unit = matches.value_of_t_or_exit::<Units>("UNIT");
        let numeric_only = matches.is_present("NUMERIC_ONLY");

        (
            ThinMetadataSizeOptions {
                nr_blocks: pool_size / block_size as u64,
                max_thins,
            },
            unit,
            numeric_only,
        )
    }
}

impl<'a> Command<'a> for ThinMetadataSizeCommand {
    fn name(&self) -> &'a str {
        "thin_metadata_size"
    }

    fn run(&self, args: &mut dyn Iterator<Item = std::ffi::OsString>) -> exitcode::ExitCode {
        let (opts, unit, numeric_only) = self.parse_args(args);

        match metadata_size(&opts) {
            Ok(size) => {
                let size = to_units(size * 512, unit);
                if numeric_only {
                    println!("{}", size);
                } else {
                    let mut name = unit.to_string();
                    name.push('s');
                    println!("{} {}", size, name);
                }
                exitcode::OK
            }
            Err(reason) => {
                eprintln!("{}", reason);
                // FIXME: use to_exit_code
                exitcode::USAGE
            }
        }
    }
}

//------------------------------------------
