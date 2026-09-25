//! Look inside any ARM64 executable, catalogued or not: symbols, a function's code, its direct
//! callers, the code that forms a string's address, and fixed-up slots such as a vtable's. No
//! game starts. Addresses in this output are for development only.
//!
//! The image is `--image PATH`, or `STELLARIS_PATH` when that is absent. A directory resolves to
//! its executable the way `Native::open` resolves an installation.
use pdx_native::internals::inspect::{Image, read_image};

const USAGE: &str = "usage: inspect [--image PATH] \
    (--symbols TEXT | --function NAME|0xADDRESS [--limit BYTES] | --callers NAME|0xADDRESS \
    | --strings TEXT | --slots NAME|0xADDRESS [--count N])";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = Arguments::parse(
        std::env::args().skip(1),
        std::env::var("STELLARIS_PATH").ok(),
    )?;
    let bytes = read_image(arguments.image.as_ref())?;
    let image = Image::read(&bytes)?;

    println!("{}", image.identity()?);
    match image.pointer_resolution() {
        Ok(()) => println!("pointer resolution: available"),
        Err(diagnostic) => println!("pointer resolution unavailable: {diagnostic}"),
    }
    println!();

    match arguments.command {
        Command::Symbols(text) => {
            for (address, name) in image.symbols(&text) {
                println!("{address:#x}  {name}");
            }
        }
        Command::Function(query) => {
            let start = image.address(&query)?;
            print!("{}", image.disassemble(start, arguments.limit)?);
        }
        Command::Callers(query) => {
            println!("direct bl and b only; calls through a register or a vtable are not found");
            for caller in image.callers(image.address(&query)?) {
                println!("{caller}");
            }
        }
        Command::Strings(text) => {
            println!("adr, or adrp then add in one function with no branch or write between them");
            for reference in image.string_references(&text) {
                println!("{reference}");
            }
        }
        Command::Slots(query) => {
            for slot in image.slots(image.address(&query)?, arguments.count)? {
                println!("{slot}");
            }
        }
    }

    Ok(())
}

enum Command {
    Symbols(String),
    Function(String),
    Callers(String),
    Strings(String),
    Slots(String),
}

struct Arguments {
    image: String,
    command: Command,
    limit: u64,
    count: usize,
}

impl Arguments {
    /// `default_image` is used when `--image` is absent.
    fn parse(
        mut arguments: impl Iterator<Item = String>,
        default_image: Option<String>,
    ) -> Result<Self, String> {
        let mut image = default_image;
        let mut command = None;
        let mut limit = 64 * 1024;
        let mut count = 16;

        while let Some(flag) = arguments.next() {
            let value = arguments.next().ok_or(USAGE)?;
            match flag.as_str() {
                "--image" => image = Some(value),
                "--symbols" => command = Some(Command::Symbols(value)),
                "--function" => command = Some(Command::Function(value)),
                "--callers" => command = Some(Command::Callers(value)),
                "--strings" => command = Some(Command::Strings(value)),
                "--slots" => command = Some(Command::Slots(value)),
                "--limit" => limit = value.parse().map_err(|_| USAGE)?,
                "--count" => count = value.parse().map_err(|_| USAGE)?,
                _ => return Err(USAGE.into()),
            }
        }

        Ok(Self {
            image: image.ok_or(USAGE)?,
            command: command.ok_or(USAGE)?,
            limit,
            count,
        })
    }
}
